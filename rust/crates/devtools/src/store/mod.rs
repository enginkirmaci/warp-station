//! The app store: one entity owning all app state; views hold only ephemeral
//! UI state, snapshot `store.read(cx)` at render top and end mutations with
//! `cx.notify()`. The store never holds UI handles.

pub mod projection;

use crate::net::{NetCommand, UiEvent};
use devtools_core::repo_store::{self, RepoEntry};
use devtools_core::settings::{AppSettings, RepoSortMode, SettingsService};
use gpui::{App, Context};
use std::sync::Arc;

pub struct Store {
    pub net: async_channel::Sender<NetCommand>,
    settings: Arc<SettingsService>,
    pub repos: Vec<RepoEntry>,
    pub search: String,
    pub tag_filter: Option<String>,
    pub sort_mode: RepoSortMode,
    /// Row order for the table: indices into `repos` after filter+sort.
    pub projected: Vec<usize>,
    /// Folder path of the repo the bottom bar is attached to.
    pub selected_repo: Option<String>,
    pub scanning: bool,
    /// Dialog-bound net results, drained by the page's render pass.
    pub deep_scan_result: Option<Vec<devtools_core::scan::ScannedRepo>>,
    pub clone_result: Option<(String, String, devtools_core::model::GitSyncResult)>,

    /// Write-behind cache saves: bumped on every tag/favorite mutation; the
    /// deferred flush only writes when its generation is still current.
    save_generation: u64,
}

impl Store {
    pub fn new(_cx: &App, net: async_channel::Sender<NetCommand>) -> Self {
        let settings = Arc::new(SettingsService::open());
        let app_settings = settings.get();
        let cached = repo_store::load_cache(&repo_store::cache_path())
            .map(|c| c.repos.iter().map(repo_store::entry_from_cache).collect())
            .unwrap_or_default();

        let mut store = Self {
            net,
            settings,
            repos: cached,
            search: String::new(),
            tag_filter: None,
            sort_mode: app_settings.repos.sort_mode,
            projected: Vec::new(),
            selected_repo: None,
            scanning: false,
            deep_scan_result: None,
            clone_result: None,
            save_generation: 0,
        };
        store.reproject();

        // Cache first for instant labels, then the session scan + a status
        // pass over whatever we know (the scan's settle re-probes the merged
        // list).
        let folders: Vec<std::path::PathBuf> =
            store.repos.iter().map(|r| r.folder_path.clone()).collect();
        let _ = store.net.send_blocking(NetCommand::ProbeAll { folders });
        store.scanning = true;
        let _ = store.net.send_blocking(NetCommand::Scan {
            settings: app_settings.repos.clone(),
            previous: store.repos.clone(),
        });
        store
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.get()
    }

    pub fn update_settings(&self, mutate: impl FnOnce(&mut AppSettings)) {
        if let Err(err) = self.settings.update(mutate) {
            tracing::warn!("could not save settings: {err}");
        }
    }

    pub fn kick_scan(&mut self, settings: &AppSettings) {
        self.scanning = true;
        let _ = self.net.send_blocking(NetCommand::Scan {
            settings: settings.repos.clone(),
            previous: self.repos.clone(),
        });
    }

    /// Manual refresh: rescan with the current settings, then re-probe.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings();
        self.kick_scan(&settings);
        cx.notify();
    }

    pub fn on_net_event(&mut self, event: UiEvent, cx: &mut Context<Self>) {
        match event {
            UiEvent::ScanSettled { repos, removed: _ } => {
                self.repos = repos;
                self.scanning = false;
                self.reproject();
                // statuses re-check after every rescan without coordination
                let folders: Vec<std::path::PathBuf> =
                    self.repos.iter().map(|r| r.folder_path.clone()).collect();
                let _ = self.net.send_blocking(NetCommand::ProbeAll { folders });
                cx.notify();
            }
            UiEvent::StatusSettled { probes } => {
                for probe in &probes {
                    if let Some(entry) = self
                        .repos
                        .iter_mut()
                        .find(|r| r.folder_path.to_string_lossy() == probe.folder_path)
                    {
                        repo_store::apply_status_probe(entry, probe);
                    }
                }
                self.reproject();
                cx.notify();
            }
            UiEvent::DeepScanSettled { found, .. } => {
                self.deep_scan_result = Some(found);
                cx.notify();
            }
            UiEvent::CloneSettled { parent_dir, name, result, .. } => {
                self.clone_result = Some((parent_dir, name, result));
                cx.notify();
            }
        }
    }

    /// Adopts scan results into the tracked list (the Add-Repositories
    /// dialog's Add button): new folders append; known folders keep their
    /// tags. Persists immediately and re-probes the new arrivals.
    pub fn register_repos(&mut self, found: &[devtools_core::scan::ScannedRepo], cx: &mut Context<Self>) {
        let previous: Vec<RepoEntry> = self.repos.clone();
        let (mut merged, _) = repo_store::merge_scan(&previous, found);
        // merge_scan drops repos absent from `found`; the dialog adds to the
        // EXISTING list, so union instead
        for existing in &previous {
            if !merged
                .iter()
                .any(|m: &RepoEntry| m.folder_path == existing.folder_path)
            {
                merged.push(existing.clone());
            }
        }
        merged.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        let new_folders: Vec<std::path::PathBuf> = merged
            .iter()
            .filter(|m| !previous.iter().any(|p| p.folder_path == m.folder_path))
            .map(|m| m.folder_path.clone())
            .collect();
        self.repos = merged;
        self.reproject();
        if let Err(err) = repo_store::save_cache(&repo_store::cache_path(), &self.repos) {
            tracing::warn!("could not save repo cache: {err}");
        }
        if !new_folders.is_empty() {
            let _ = self.net.send_blocking(NetCommand::ProbeAll { folders: new_folders });
        }
        cx.notify();
    }

    /// Adopts a finished clone: register its folder and select it.
    pub fn adopt_clone(&mut self, parent_dir: &str, name: &str, cx: &mut Context<Self>) {
        let folder = std::path::PathBuf::from(parent_dir).join(name);
        if !folder.join(".git").is_dir() {
            tracing::warn!("clone destination missing after clone: {}", folder.display());
            return;
        }
        let scanned = vec![devtools_core::scan::ScannedRepo {
            name: name.to_string(),
            folder_path: folder.clone(),
            solution_path: None,
            tags: Vec::new(),
        }];
        self.register_repos(&scanned, cx);
        self.select_repo(Some(folder.to_string_lossy().to_string()), cx);
    }

    // ---- list shaping ------------------------------------------------------

    pub fn set_search(&mut self, text: String, cx: &mut Context<Self>) {
        self.search = text;
        self.reproject();
        cx.notify();
    }

    pub fn set_sort(&mut self, mode: RepoSortMode, cx: &mut Context<Self>) {
        self.sort_mode = mode;
        self.update_settings(|s| s.repos.sort_mode = mode);
        self.reproject();
        cx.notify();
    }

    pub fn toggle_tag_filter(&mut self, tag: &str, cx: &mut Context<Self>) {
        self.tag_filter = if self.tag_filter.as_deref() == Some(tag) {
            None
        } else {
            Some(tag.to_string())
        };
        self.reproject();
        cx.notify();
    }

    pub fn select_repo(&mut self, folder: Option<String>, cx: &mut Context<Self>) {
        self.selected_repo = folder;
        cx.notify();
    }

    pub fn selected_entry(&self) -> Option<&RepoEntry> {
        self.selected_repo
            .as_ref()
            .and_then(|folder| self.repos.iter().find(|r| r.folder_path.to_string_lossy() == *folder))
    }

    /// Replaces the whole list (test setup; later also the Add-Repositories
    /// dialog's adoption path).
    pub fn replace_repos(&mut self, repos: Vec<RepoEntry>) {
        self.repos = repos;
        self.reproject();
    }

    fn reproject(&mut self) {
        self.projected =
            projection::project(&self.repos, &self.search, self.tag_filter.as_deref(), self.sort_mode);
    }

    /// Every distinct tag across the list, favorites and platform leading —
    /// the filter chip row.
    pub fn all_tags(&self) -> Vec<String> {
        projection::all_tags(&self.repos)
    }

    // ---- tag mutations (write-behind) ---------------------------------------

    pub fn toggle_favorite(&mut self, folder: &str, cx: &mut Context<Self>) {
        self.toggle_repo_tag_inner(folder, repo_store::FAVORITES_TAG, cx);
    }

    pub fn toggle_repo_tag(&mut self, folder: &str, tag: &str, cx: &mut Context<Self>) {
        if tag.eq_ignore_ascii_case(repo_store::FAVORITES_TAG)
            || tag.eq_ignore_ascii_case(devtools_core::scan::PLATFORM_TAG)
        {
            return;
        }
        self.toggle_repo_tag_inner(folder, tag, cx);
    }

    fn toggle_repo_tag_inner(&mut self, folder: &str, tag: &str, cx: &mut Context<Self>) {
        let Some(entry) = self
            .repos
            .iter_mut()
            .find(|r| r.folder_path.to_string_lossy() == folder)
        else {
            return;
        };
        let existing = entry.tags.iter().position(|t| t.eq_ignore_ascii_case(tag));
        match existing {
            Some(ix) => {
                entry.tags.remove(ix);
            }
            None => entry.tags.push(tag.to_string()),
        }
        self.reproject();
        self.save_generation += 1;
        let generation = self.save_generation;
        let repos = self.repos.clone();
        cx.spawn(async move |this, cx| {
            // a burst of star/tag clicks collapses into one full-file write
            cx.background_executor()
                .timer(std::time::Duration::from_millis(400))
                .await;
            let _ = this.update(cx, |store: &mut Store, _| {
                if store.save_generation == generation {
                    if let Err(err) = repo_store::save_cache(&repo_store::cache_path(), &repos) {
                        tracing::warn!("could not save repo cache: {err}");
                    }
                    store.save_generation = 0;
                }
            });
        })
        .detach();
        cx.notify();
    }
}
