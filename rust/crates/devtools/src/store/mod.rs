//! The app store: one entity owning all app state; views hold only ephemeral
//! UI state, snapshot `store.read(cx)` at render top and end mutations with
//! `cx.notify()`. The store never holds UI handles.

pub mod projection;

use crate::net::{GitAction, NetCommand, UiEvent, WandKind};
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

    // ---- bottom bar (the selected repo's workspace) ----
    pub selected_tab: BottomBarTab,
    pub changes: Option<devtools_core::model::GitChangeSnapshot>,
    pub changes_loaded_for: Option<String>,
    pub branches: Vec<devtools_core::model::GitBranchRef>,
    pub recent_commits: Vec<devtools_core::model::GitCommitInfo>,
    pub unpushed: Option<std::collections::HashSet<String>>,
    pub git_busy: bool,
    pub last_git_error: Option<String>,
    /// One freshly loaded change-row patch, drained by the Changes view.
    pub change_patch_result: Option<(String, bool, String)>,
    pub history: Vec<devtools_core::model::GitCommitInfo>,
    /// GitHub activity per folder — feeds the PR/Issues tabs; the entry
    /// counts on the repos table come from these snapshots.
    pub github_by_folder: std::collections::HashMap<String, devtools_core::github::GitHubSnapshot>,
    pub azure_by_folder: std::collections::HashMap<String, devtools_core::azure::AzureSnapshot>,
    pub github_details: Option<devtools_core::github::GitHubRepoDetails>,

    // ---- wand (opencode) state ----
    pub wand_busy: Option<WandKind>,
    /// A pending permission ask: (wand id, ask id, title, seconds left on the
    /// auto-reject countdown).
    pub wand_ask: Option<(String, String, String, u32)>,
    pub wand_error: Option<String>,
    /// The commit-message wand's draft, drained into the Changes commit box.
    pub commit_message_draft: Option<String>,
    /// The README wand's draft, rendered by the Overview pane with Save/Discard.
    pub readme_draft: Option<String>,
    /// The opencode model catalog (settings pickers + wands).
    pub models: Vec<String>,
    pub summary_busy: bool,
    /// Transient user-facing message (summary written, wand failed…).
    pub notice: Option<(String, std::time::Instant)>,
    /// A note to open in the Notes window (global search click), drained by
    /// the Notes page's render.
    pub pending_note_open: Option<std::path::PathBuf>,
    /// Global-search results (repos by name/path + note content hits).
    pub global_search: Vec<GlobalSearchHit>,
    /// Drill-down for the selected history commit, drained by the view.
    pub commit_details: Option<devtools_core::model::GitCommitDetails>,
    pub commit_file_patch: Option<(String, String)>,

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
            selected_tab: BottomBarTab::Changes,
            changes: None,
            changes_loaded_for: None,
            branches: Vec::new(),
            recent_commits: Vec::new(),
            unpushed: None,
            git_busy: false,
            last_git_error: None,
            change_patch_result: None,
            history: Vec::new(),
            github_by_folder: std::collections::HashMap::new(),
            azure_by_folder: std::collections::HashMap::new(),
            github_details: None,
            wand_busy: None,
            wand_ask: None,
            wand_error: None,
            commit_message_draft: None,
            readme_draft: None,
            models: Vec::new(),
            summary_busy: false,
            notice: None,
            pending_note_open: None,
            global_search: Vec::new(),
            commit_details: None,
            commit_file_patch: None,
            save_generation: 0,
        };
        store.reproject();

        // Cache first for instant labels, then the session scan + a status
        // pass over whatever we know (the scan's settle re-probes the merged
        // list).
        let folders: Vec<std::path::PathBuf> =
            store.repos.iter().map(|r| r.folder_path.clone()).collect();
        let _ = store.net.send_blocking(NetCommand::ProbeAll { folders: folders.clone() });
        store.kick_github_pass(&folders);
        store.kick_azure_pass(&folders);
        store.scanning = true;
        let _ = store.net.send_blocking(NetCommand::Scan {
            settings: app_settings.repos.clone(),
            previous: store.repos.clone(),
        });

        // E2E hook: DEVTOOLS_SELECT=<index-or-name-substring> opens the bottom
        // bar on launch so automation can verify the workspace surfaces
        if let Ok(pattern) = std::env::var("DEVTOOLS_SELECT") {
            if let Some(entry) = store
                .repos
                .iter()
                .find(|r| r.name.eq_ignore_ascii_case(&pattern))
                .or_else(|| store.repos.get(pattern.parse::<usize>().unwrap_or(usize::MAX)))
            {
                store.selected_repo = Some(entry.folder_path.to_string_lossy().to_string());
                let _ = store.net.send_blocking(NetCommand::LoadChanges {
                    folder: entry.folder_path.clone(),
                });
            }
        }
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
                let _ = self.net.send_blocking(NetCommand::ProbeAll { folders: folders.clone() });
                self.kick_github_pass(&folders);
                self.kick_azure_pass(&folders);
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
            UiEvent::ChangesLoaded { folder, snapshot, branches, recent, unpushed } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.changes = snapshot;
                    self.changes_loaded_for = Some(folder);
                    self.branches = branches;
                    if !recent.is_empty() {
                        self.recent_commits = recent;
                    }
                    self.unpushed = unpushed;
                }
                cx.notify();
            }
            UiEvent::ChangePatchLoaded { folder, path, staged, patch } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.change_patch_result = Some((path, staged, patch));
                }
                cx.notify();
            }
            UiEvent::HistoryLoaded { folder, commits, unpushed } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.history = commits;
                    self.unpushed = unpushed;
                }
                cx.notify();
            }
            UiEvent::AzureSettled { items } => {
                for (folder, snapshot) in items {
                    self.azure_by_folder.insert(folder, snapshot);
                }
                self.reproject();
                cx.notify();
            }
            UiEvent::GitHubSettled { items } => {
                for (folder, snapshot) in items {
                    if let Some(entry) = self
                        .repos
                        .iter_mut()
                        .find(|r| r.folder_path.to_string_lossy() == folder)
                    {
                        entry.github_pr_count = snapshot.pulls.len() as u32;
                        entry.github_issue_count = snapshot.issues.len() as u32;
                    }
                    self.github_by_folder.insert(folder, snapshot);
                }
                self.reproject();
                cx.notify();
            }
            UiEvent::GitHubDetailsLoaded { folder, details } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.github_details = details;
                }
                cx.notify();
            }
            UiEvent::ModelsLoaded { models } => {
                self.models = models;
                cx.notify();
            }
            UiEvent::SummaryDone { result, .. } => {
                self.summary_busy = false;
                self.notice = Some(match result {
                    Ok(path) => (format!("summary written: {path}"), std::time::Instant::now()),
                    Err(err) => (format!("summary failed: {err}"), std::time::Instant::now()),
                });
                cx.notify();
            }
            UiEvent::WandAsk { wand_id, ask_id, title } => {
                self.wand_ask = Some((wand_id, ask_id, title, 15));
                cx.notify();
            }
            UiEvent::WandLog { line, .. } => {
                tracing::info!("wand: {line}");
            }
            UiEvent::WandDone { kind, result, .. } => {
                self.wand_busy = None;
                self.wand_ask = None;
                match (&result, kind) {
                    (Ok(text), WandKind::Commit) => {
                        self.commit_message_draft = Some(text.trim().to_string());
                    }
                    (Ok(text), WandKind::Readme) => {
                        self.readme_draft = Some(text.trim().to_string());
                    }
                    (Err(err), _) => {
                        tracing::warn!("wand failed: {err}");
                        self.wand_error = Some(err.clone());
                    }
                }
                cx.notify();
            }
            UiEvent::CommitDetailsLoaded { folder, details } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.commit_details = details;
                }
                cx.notify();
            }
            UiEvent::CommitFilePatchLoaded { folder, hash, path, patch } => {
                if self.selected_repo.as_deref() == Some(folder.as_str()) {
                    self.commit_file_patch = Some((format!("{hash}|{path}"), patch));
                }
                cx.notify();
            }
            UiEvent::GitDone { folder, label, error, commit_hash } => {
                self.git_busy = false;
                if let Some(err) = error {
                    tracing::warn!("git {label} failed in {folder}: {err}");
                    self.last_git_error = Some(err);
                } else {
                    self.last_git_error = None;
                    if let Some(hash) = commit_hash {
                        tracing::info!("committed {hash} in {folder}");
                    }
                }
                cx.notify();
            }
        }
    }

    /// Fires the GitHub activity pass when the column is enabled and gh
    /// resolves; a disabled column spawns no processes at all.
    pub fn kick_github_pass(&mut self, folders: &[std::path::PathBuf]) {
        let settings = self.settings();
        if !settings.repos.enable_git_hub || folders.is_empty() {
            return;
        }
        let Some(gh_path) = devtools_core::exe::locate(&settings.repos.git_hub_executable) else {
            tracing::warn!(
                "GitHub column enabled but the gh CLI could not be located ({})",
                settings.repos.git_hub_executable
            );
            return;
        };
        let _ = self.net.send_blocking(NetCommand::GitHubPass {
            folders: folders.to_vec(),
            gh_exe: gh_path.to_string_lossy().to_string(),
        });
    }

    /// Fires the Azure DevOps pass when the column is enabled and a PAT
    /// resolves (settings or the environment fallbacks).
    pub fn kick_azure_pass(&mut self, folders: &[std::path::PathBuf]) {
        let settings = self.settings();
        if !settings.repos.enable_azure_dev_ops || folders.is_empty() {
            return;
        }
        let Some(pat) = devtools_core::azure::resolve_pat(&settings.repos.azure_dev_ops_pat) else {
            tracing::warn!(
                "Azure DevOps column enabled but no personal access token is configured; set one in Repos settings (or the AZURE_DEVOPS_PAT environment variable)"
            );
            return;
        };
        let _ = self.net.send_blocking(NetCommand::AzurePass {
            folders: folders.to_vec(),
            pat,
            custom_base: settings.repos.azure_dev_ops_url.clone(),
        });
    }

    /// Kicks the yesterday-summary wand across every tracked repo.
    pub fn run_summary(&mut self, cx: &mut Context<Self>) {
        if self.summary_busy {
            return;
        }
        let settings = self.settings();
        if !settings.opencode.enable_opencode {
            self.notice = Some(("OpenCode is disabled in settings".into(), std::time::Instant::now()));
            cx.notify();
            return;
        }
        let Some(exe) = devtools_core::exe::locate(&settings.repos.opencode_executable) else {
            self.notice = Some(("opencode executable not found".into(), std::time::Instant::now()));
            cx.notify();
            return;
        };
        let model = {
            let m = settings.opencode.default_model.clone();
            (!m.is_empty()).then_some(m)
        };
        let folders: Vec<(String, std::path::PathBuf)> = self
            .repos
            .iter()
            .map(|r| (r.name.clone(), r.folder_path.clone()))
            .collect();
        if folders.is_empty() {
            return;
        }
        let summaries_dir = devtools_core::notes::resolve_store_root(&settings.general.notes_store_path)
            .join("Summaries");
        let day = chrono::Local::now().date_naive() - chrono::Duration::days(1);
        self.summary_busy = true;
        let id = format!("summary-{}", self.save_generation);
        self.save_generation += 1;
        let _ = self.net.send_blocking(NetCommand::WandSummary {
            id,
            folders,
            day,
            model,
            opencode_exe: exe.to_string_lossy().to_string(),
            summaries_dir,
        });
        cx.notify();
    }

    /// Loads the model catalog (cache-backed; the CLI runs on the net thread).
    pub fn load_models(&mut self, cx: &mut Context<Self>) {
        let settings = self.settings();
        if !settings.opencode.enable_opencode {
            self.models = devtools_core::opencode::cached_models("");
            cx.notify();
            return;
        }
        if let Some(exe) = devtools_core::exe::locate(&settings.repos.opencode_executable) {
            let _ = self.net.send_blocking(NetCommand::LoadModels {
                opencode_exe: exe.to_string_lossy().to_string(),
                default_model: settings.opencode.default_model.clone(),
            });
        } else {
            self.models = devtools_core::opencode::cached_models(&settings.opencode.default_model);
            cx.notify();
        }
    }

    /// Kicks one wand run for the selected repo.
    pub fn run_wand(&mut self, kind: WandKind, cx: &mut Context<Self>) {
        if self.wand_busy.is_some() {
            return;
        }
        let Some(folder) = self.selected_repo.clone() else { return };
        let settings = self.settings();
        if !settings.opencode.enable_opencode {
            self.wand_error = Some("OpenCode is disabled in settings".into());
            cx.notify();
            return;
        }
        let Some(exe) = devtools_core::exe::locate(&settings.repos.opencode_executable) else {
            self.wand_error = Some("opencode executable not found".into());
            cx.notify();
            return;
        };
        let model = match kind {
            WandKind::Commit => settings
                .opencode
                .commit_model
                .clone()
                .filter(|m| !m.is_empty())
                .or_else(|| {
                    let m = settings.opencode.default_model.clone();
                    (!m.is_empty()).then_some(m)
                }),
            WandKind::Readme => {
                let m = settings.opencode.default_model.clone();
                (!m.is_empty()).then_some(m)
            }
        };
        self.wand_busy = Some(kind);
        self.wand_error = None;
        let id = format!("wand-{}", self.save_generation);
        self.save_generation += 1;
        let _ = self.net.send_blocking(NetCommand::Wand {
            id,
            kind,
            folder: std::path::PathBuf::from(folder),
            model,
            opencode_exe: exe.to_string_lossy().to_string(),
        });
        cx.notify();
    }

    /// Answers a pending permission ask.
    pub fn respond_wand(&mut self, response: &str, cx: &mut Context<Self>) {
        if let Some((wand_id, ask_id, _, _)) = self.wand_ask.clone() {
            self.wand_ask = None;
            let _ = self.net.send_blocking(NetCommand::WandRespond {
                wand_id,
                ask_id,
                response: response.to_string(),
            });
            cx.notify();
        }
    }

    /// Once-per-second countdown tick: auto-rejects a pending ask at zero.
    pub fn tick_wand(&mut self, cx: &mut Context<Self>) {
        if let Some((wand, ask, title, secs)) = self.wand_ask.clone() {
            if secs <= 1 {
                self.wand_ask = None;
                let _ = self.net.send_blocking(NetCommand::WandRespond {
                    wand_id: wand,
                    ask_id: ask,
                    response: "reject".into(),
                });
            } else {
                self.wand_ask = Some((wand, ask, title, secs - 1));
            }
            cx.notify();
        }
    }

    /// Sends one mutation for the selected repo and marks the rail busy; the
    /// net thread settles the reload.
    pub fn run_git(&mut self, action: GitAction, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_repo.clone() else { return };
        self.git_busy = true;
        self.last_git_error = None;
        let _ = self.net.send_blocking(NetCommand::Git {
            folder: std::path::PathBuf::from(folder),
            action,
        });
        cx.notify();
    }

    pub fn set_tab(&mut self, tab: BottomBarTab, cx: &mut Context<Self>) {
        self.selected_tab = tab;
        cx.notify();
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
        if self.selected_repo == folder {
            return;
        }
        self.selected_repo = folder;
        self.changes = None;
        self.changes_loaded_for = None;
        self.branches.clear();
        self.recent_commits.clear();
        self.history.clear();
        self.unpushed = None;
        self.last_git_error = None;
        self.commit_details = None;
        if let Some(folder) = &self.selected_repo {
            let _ = self.net.send_blocking(NetCommand::LoadChanges {
                folder: std::path::PathBuf::from(folder),
            });
        }
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

#[derive(Clone, Debug)]
pub enum GlobalSearchHit {
    Repo { name: String, folder: String },
    Note { title: String, excerpt: String, path: std::path::PathBuf },
}

impl Store {
    /// One global search: repo name/path matches plus capped note-content
    /// hits across every tracked repo's notes root.
    pub fn run_global_search(&mut self, term: &str, cx: &mut Context<Self>) {
        let term = term.trim();
        if term.is_empty() {
            self.global_search.clear();
            cx.notify();
            return;
        }
        let mut hits: Vec<GlobalSearchHit> = self
            .repos
            .iter()
            .filter(|repo| {
                repo.name.to_lowercase().contains(&term.to_lowercase())
                    || repo.folder_path.to_string_lossy().to_lowercase().contains(&term.to_lowercase())
            })
            .map(|repo| GlobalSearchHit::Repo {
                name: repo.name.clone(),
                folder: repo.folder_path.to_string_lossy().to_string(),
            })
            .collect();

        // note content: the settings' store root, per-repo subfolders
        let settings = self.settings();
        let root = devtools_core::notes::resolve_store_root(&settings.general.notes_store_path);
        for repo in &self.repos {
            if hits.len() > 20 {
                break;
            }
            let notes_root = devtools_core::notes::repo_notes_root(&root, &repo.name);
            for hit in devtools_core::notes::search(&notes_root, term) {
                hits.push(GlobalSearchHit::Note {
                    title: hit.title,
                    excerpt: hit.excerpt,
                    path: hit.path,
                });
                if hits.len() > 20 {
                    break;
                }
            }
        }
        self.global_search = hits;
        cx.notify();
    }
}

/// The bottom bar's tabs. Azure is labelled Pipelines in the UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BottomBarTab {
    Overview,
    Changes,
    PullRequests,
    Issues,
    Azure,
}

impl BottomBarTab {
    pub fn label(&self) -> &'static str {
        match self {
            BottomBarTab::Overview => "Overview",
            BottomBarTab::Changes => "Changes",
            BottomBarTab::PullRequests => "Pull Requests",
            BottomBarTab::Issues => "Issues",
            BottomBarTab::Azure => "Pipelines",
        }
    }
}
