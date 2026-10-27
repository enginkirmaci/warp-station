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
    /// Checked tag-filter chips (OR semantics: a repo matches when it has any).
    pub tag_filters: Vec<String>,
    pub sort_mode: RepoSortMode,
    /// false = the mode's default "best-first" read, true = flipped. Seeded
    /// from the persisted direction when set, else the mode's default.
    pub sort_ascending: bool,
    /// Row order for the table: indices into `repos` after filter+sort.
    pub projected: Vec<usize>,
    /// Folder path of the repo the bottom bar is attached to.
    pub selected_repo: Option<String>,
    pub scanning: bool,
    /// Dialog-bound net results, drained by the page's render pass.
    pub deep_scan_result: Option<Vec<devtools_core::scan::ScannedRepo>>,
    pub clone_result: Option<(String, String, devtools_core::model::GitSyncResult)>,
    /// The Browse… folder picker's answer, drained into the Add-Repositories
    /// dialog's path field.
    pub folder_pick_result: Option<String>,

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
    /// The repo the running wand was kicked for — its result may only land
    /// while that repo is still selected.
    pub wand_repo: Option<String>,
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
    /// Toast stack (newest last), capped; the window overlay renders it and
    /// the per-second tick expires it.
    pub notices: Vec<Notice>,
    notice_generation: u64,
    /// A note to open in the Notes window (global search click), drained by
    /// the Notes page's render.
    pub pending_note_open: Option<std::path::PathBuf>,
    /// Global-search results (repos by name/path + note content hits).
    pub global_search: Vec<GlobalSearchHit>,
    /// The repos-table notes button asks for the floating notes window.
    pub notes_requested: bool,
    /// Drill-down for the selected history commit, drained by the view.
    pub commit_details: Option<devtools_core::model::GitCommitDetails>,
    pub commit_file_patch: Option<(String, String)>,

    /// Write-behind cache saves: bumped on every tag/favorite mutation; the
    /// deferred flush only writes when its generation is still current.
    save_generation: u64,
    /// Bottom-panel height in logical px, shared by every tab (drag the top edge).
    pub bar_height: f32,
    /// Latest NuGet watch service snapshot (watching, count, activity log).
    pub nuget: devtools_core::nuget::NugetSnapshot,
}

impl Store {
    pub fn new(_cx: &App, net: async_channel::Sender<NetCommand>) -> Self {
        Self::with_settings(net, Arc::new(SettingsService::open()))
    }

    /// The probe seam: probes pass a scratch-backed service so a settings
    /// mutation never lands in the real ~/.devtools/settings.json.
    pub fn with_settings(net: async_channel::Sender<NetCommand>, settings: Arc<SettingsService>) -> Self {
        let app_settings = settings.get();
        let cached = repo_store::load_cache(&repo_store::cache_path())
            .map(|c| c.repos.iter().map(repo_store::entry_from_cache).collect())
            .unwrap_or_default();

        let mut store = Self {
            net,
            settings,
            repos: cached,
            tag_filters: Vec::new(),
            sort_mode: app_settings.repos.sort_mode,
            sort_ascending: app_settings
                .repos
                .sort_ascending
                .unwrap_or_else(|| app_settings.repos.sort_mode.default_ascending()),
            projected: Vec::new(),
            selected_repo: None,
            bar_height: 440.0,
            scanning: false,
            deep_scan_result: None,
            clone_result: None,
            folder_pick_result: None,
            selected_tab: BottomBarTab::Overview,
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
            wand_repo: None,
            wand_ask: None,
            wand_error: None,
            commit_message_draft: None,
            readme_draft: None,
            models: Vec::new(),
            summary_busy: false,
            notices: Vec::new(),
            notice_generation: 0,
            pending_note_open: None,
            global_search: Vec::new(),
            notes_requested: false,
            commit_details: None,
            commit_file_patch: None,
            save_generation: 0,
            nuget: Default::default(),
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
        if let Ok(pattern) = std::env::var("DEVTOOLS_SELECT")
            && let Some(entry) = store
                .repos
                .iter()
                .find(|r| r.name.eq_ignore_ascii_case(&pattern))
                .or_else(|| store.repos.get(pattern.parse::<usize>().unwrap_or(usize::MAX)))
            {
                store.selected_repo = Some(entry.folder_path.to_string_lossy().to_string());
                let _ = store.net.send_blocking(NetCommand::LoadChanges {
                    folder: entry.folder_path.clone(),
                });
                // companion hook: DEVTOOLS_TAB=<tab label> picks the open tab
                if let Ok(tab) = std::env::var("DEVTOOLS_TAB") {
                    store.selected_tab = BottomBarTab::parse(&tab);
                }
            }
        // Notes stay closed until the user asks for them (titlebar button,
        // repo-row note icon, tools menu, or a global-search hit).
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

    // ---- NuGet Local watch controls (config captured at press time) ----

    pub fn start_nuget(&mut self) {
        let section = self.settings().nuget_local;
        let _ = self.net.send_blocking(NetCommand::NugetStart {
            config: devtools_core::nuget::NugetConfig {
                enable: section.enable_nuget,
                watch_folder: section.watch_folder,
                filter: section.nuget_package_filter,
                copy_delay: std::time::Duration::from_millis(u64::from(section.file_copy_delay_ms)),
                count_window: std::time::Duration::from_secs(u64::from(section.count_reset_interval_seconds)),
            },
        });
    }

    pub fn stop_nuget(&mut self) {
        let _ = self.net.send_blocking(NetCommand::NugetStop);
    }

    pub fn register_nuget_source(&mut self) {
        let _ = self.net.send_blocking(NetCommand::NugetRegister);
    }

    /// Persists the watch folder and applies it to a running watch.
    pub fn set_nuget_folder(&mut self, folder: String) {
        let trimmed = folder.trim().to_string();
        self.update_settings(|s| {
            s.nuget_local.watch_folder = (!trimmed.is_empty()).then(|| trimmed.clone());
        });
        let _ = self.net.send_blocking(NetCommand::NugetSetFolder { folder: trimmed });
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
            UiEvent::FolderPicked { path } => {
                self.folder_pick_result = path;
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
            UiEvent::Nuget(snapshot) => {
                self.nuget = snapshot;
                cx.notify();
            }
            UiEvent::ClipboardHotkey { message } => {
                // the copy ran on the trigger thread (gpui's Wayland
                // clipboard write is focus-gated); here we only toast
                let kind = NoticeKind::for_copy_message(message);
                self.notify(kind, message.to_string());
                cx.notify();
            }
            UiEvent::SummaryDone { result, .. } => {
                self.summary_busy = false;
                match result {
                    Ok(path) => self.notify_success(format!("summary written: {path}")),
                    Err(err) => self.notify_error(format!("summary failed: {err}")),
                }
                cx.notify();
            }
            UiEvent::WandAsk { wand_id, ask_id, title } => {
                // the auto-allow setting answers without ever raising the
                // popup ("always" whitelists the pattern for this run only)
                if self.settings().opencode.auto_allow_permissions {
                    let _ = self.net.send_blocking(NetCommand::WandRespond {
                        wand_id,
                        ask_id,
                        response: "always".into(),
                    });
                } else {
                    self.wand_ask = Some((wand_id, ask_id, title, 15));
                }
                cx.notify();
            }
            UiEvent::WandLog { line, .. } => {
                tracing::info!("wand: {line}");
            }
            UiEvent::WandDone { kind, result, .. } => {
                // a run kicked for repo A must never land its draft once
                // another repo is selected
                let for_selected = self.wand_repo.as_deref() == self.selected_repo.as_deref();
                self.wand_busy = None;
                self.wand_repo = None;
                self.wand_ask = None;
                match (&result, kind) {
                    (Ok(text), WandKind::Commit) => {
                        if for_selected {
                            self.commit_message_draft = Some(text.trim().to_string());
                        }
                    }
                    (Ok(text), WandKind::Readme) => {
                        if for_selected {
                            self.readme_draft = Some(text.trim().to_string());
                        }
                    }
                    (Err(err), WandKind::Commit) => {
                        tracing::warn!("wand failed: {err}");
                        if for_selected {
                            self.wand_error = Some(err.clone());
                        }
                        self.notify_error("Could not generate a commit message");
                    }
                    (Err(err), WandKind::Readme) => {
                        tracing::warn!("wand failed: {err}");
                        if for_selected {
                            self.wand_error = Some(err.clone());
                        }
                        self.notify_error("Could not generate a README");
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
            UiEvent::GitDone { folder, label, error, commit_hash, detail } => {
                self.git_busy = false;
                let name = self
                    .repos
                    .iter()
                    .find(|r| r.folder_path.to_string_lossy() == folder)
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| folder.clone());
                if let Some(err) = error {
                    tracing::warn!("git {label} failed in {folder}: {err}");
                    self.last_git_error = Some(err.clone());
                    self.notify_error(format!(
                        "{} failed for {}: {err}",
                        capitalized_label(label),
                        name
                    ));
                } else {
                    self.last_git_error = None;
                    self.notify_success(
                        match (label, commit_hash.as_deref()) {
                            ("commit", Some(hash)) => format!("Committed {hash} to {name}"),
                            ("fetch", _) => format!("Fetched {name}"),
                            ("pull", _) => format!("Pulled {name}"),
                            ("push", _) => format!("Pushed {name}"),
                            ("sync", _) => format!("Synced {name}"),
                            ("publish", _) => format!(
                                "Published {} — it tracks the remote",
                                detail.as_deref().unwrap_or(&name)
                            ),
                            ("checkout", _) => format!(
                                "Checked out {} in {name}",
                                detail.as_deref().unwrap_or(&name)
                            ),
                            ("create branch", _) => format!(
                                "Created {} in {name}",
                                detail.as_deref().unwrap_or(&name)
                            ),
                            ("revert", _) => format!("Reverted in {name}"),
                            _ => format!("{label} ok"),
                        },
                    );
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
            self.notify_error("OpenCode is disabled in settings");
            cx.notify();
            return;
        }
        let Some(exe) = devtools_core::exe::locate(&settings.repos.opencode_executable) else {
            self.notify_error("opencode executable not found");
            cx.notify();
            return;
        };
        let model = Self::wand_model(&settings);
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

    /// Every one-shot wand (commit message, README, summary) runs on the
    /// commit model when set, falling back to the default model.
    fn wand_model(settings: &AppSettings) -> Option<String> {
        settings
            .opencode
            .commit_model
            .clone()
            .filter(|m| !m.is_empty())
            .or_else(|| {
                let m = settings.opencode.default_model.clone();
                (!m.is_empty()).then_some(m)
            })
    }

    /// Kicks one wand run for the selected repo.
    pub fn run_wand(&mut self, kind: WandKind, cx: &mut Context<Self>) {
        if self.wand_busy.is_some() {
            return;
        }
        let Some(folder) = self.selected_repo.clone() else { return };
        let settings = self.settings();
        if !settings.opencode.enable_opencode {
            self.notify_error("Could not generate — OpenCode is disabled in settings");
            cx.notify();
            return;
        }
        let Some(exe) = devtools_core::exe::locate(&settings.repos.opencode_executable) else {
            self.notify_error("Could not generate — opencode executable not found");
            cx.notify();
            return;
        };
        let model = Self::wand_model(&settings);
        self.wand_busy = Some(kind);
        self.wand_repo = Some(folder.clone());
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
        merged.sort_by_key(|a| a.name.to_lowercase());
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

    /// Adopts a finished clone: its folder joins the persisted scan roots
    /// (the roots are repo folders — this is how the rescan keeps finding it),
    /// the list re-scans, and the bar opens on the clone's Overview.
    pub fn adopt_clone(&mut self, parent_dir: &str, name: &str, cx: &mut Context<Self>) {
        let folder = std::path::PathBuf::from(parent_dir).join(name);
        if !folder.join(".git").is_dir() {
            tracing::warn!("clone destination missing after clone: {}", folder.display());
            self.notify_error("The clone landed but could not be added to the list");
            cx.notify();
            return;
        }
        let folder_str = folder.to_string_lossy().to_string();
        self.update_settings(|s| {
            if let Some(roots) = &mut s.repos.repo_scan_folders {
                if !roots.iter().any(|f| devtools_core::paths::same_folder_text(f, &folder_str)) {
                    roots.push(folder_str.clone());
                    roots.sort();
                }
            } else {
                s.repos.repo_scan_folders = Some(vec![folder_str.clone()]);
            }
        });
        let scanned = vec![devtools_core::scan::ScannedRepo {
            name: name.to_string(),
            folder_path: folder.clone(),
            solution_path: None,
            tags: Vec::new(),
        }];
        self.register_repos(&scanned, cx);
        self.kick_scan(&self.settings());
        self.open_repo_tab(&folder_str, BottomBarTab::Overview, cx);
    }

    // ---- toasts -------------------------------------------------------------

    pub fn notify(&mut self, kind: NoticeKind, text: impl Into<String>) {
        let id = self.notice_generation;
        self.notice_generation += 1;
        self.notices.push(Notice {
            id,
            kind,
            text: text.into(),
            born: std::time::Instant::now(),
        });
        // a burst of failures must not stack a wall of cards
        if self.notices.len() > 4 {
            self.notices.remove(0);
        }
    }

    pub fn notify_success(&mut self, text: impl Into<String>) {
        self.notify(NoticeKind::Success, text);
    }

    pub fn notify_info(&mut self, text: impl Into<String>) {
        self.notify(NoticeKind::Info, text);
    }

    pub fn notify_error(&mut self, text: impl Into<String>) {
        self.notify(NoticeKind::Error, text);
    }

    /// The toast's close button.
    pub fn dismiss_notice(&mut self, id: u64, cx: &mut Context<Self>) {
        let before = self.notices.len();
        self.notices.retain(|n| n.id != id);
        if self.notices.len() != before {
            cx.notify();
        }
    }

    /// Once-per-second tick: drops toasts past their kind's ttl, notifying
    /// only when one actually expired (idle windows stay idle).
    pub fn tick_notices(&mut self, cx: &mut Context<Self>) {
        let before = self.notices.len();
        self.notices.retain(|n| n.born.elapsed() < n.kind.ttl());
        if self.notices.len() != before {
            cx.notify();
        }
    }

    // ---- list shaping ------------------------------------------------------

    /// Clears the tag filter in one transition (the toolbar's Clear).
    pub fn clear_filters(&mut self, cx: &mut Context<Self>) {
        self.tag_filters.clear();
        self.reproject();
        cx.notify();
    }

    /// A sortable header click: the kit's cycle drives both the mode and the
    /// direction, so both land together.
    pub fn set_sort_direction(&mut self, mode: RepoSortMode, ascending: bool, cx: &mut Context<Self>) {
        self.sort_mode = mode;
        self.sort_ascending = ascending;
        self.update_settings(|s| {
            s.repos.sort_mode = mode;
            s.repos.sort_ascending = Some(ascending);
        });
        self.reproject();
        cx.notify();
    }

    /// Checks/unchecks one tag-filter chip (OR semantics across the set).
    pub fn toggle_tag_filter(&mut self, tag: &str, cx: &mut Context<Self>) {
        match self.tag_filters.iter().position(|t| t.eq_ignore_ascii_case(tag)) {
            Some(ix) => {
                self.tag_filters.remove(ix);
            }
            None => self.tag_filters.push(tag.to_string()),
        }
        self.reproject();
        cx.notify();
    }

    /// Row press: selects the repo and opens the bar on Overview; pressing
    /// the already-highlighted row closes the bar.
    /// Chip presses route through [open_repo_tab] instead.
    pub fn select_repo(&mut self, folder: Option<String>, cx: &mut Context<Self>) {
        if self.selected_repo.as_deref() == folder.as_deref() {
            // same row pressed again → close the bar entirely
            self.close_bar(cx);
            return;
        }
        self.selected_repo = folder;
        self.selected_tab = BottomBarTab::Overview;
        self.reset_repo_workspace();
        if let Some(folder) = &self.selected_repo {
            let _ = self.net.send_blocking(NetCommand::LoadChanges {
                folder: std::path::PathBuf::from(folder),
            });
        }
        cx.notify();
    }

    /// Selects the repo (if needed) and lands the bar on one specific tab —
    /// the table's clickable chips (branch, changes count, PR/issue/pipeline
    /// cells) come through here.
    pub fn open_repo_tab(&mut self, folder: &str, tab: BottomBarTab, cx: &mut Context<Self>) {
        if self.selected_repo.as_deref() != Some(folder) {
            self.selected_repo = Some(folder.to_string());
            self.selected_tab = tab;
            self.reset_repo_workspace();
            let _ = self.net.send_blocking(NetCommand::LoadChanges {
                folder: std::path::PathBuf::from(folder),
            });
        } else {
            self.selected_tab = tab;
        }
        cx.notify();
    }

    /// Selects the repo without the row-press toggle (the row's notes button
    /// must keep the bar open even when the repo is already selected).
    pub fn ensure_repo_selected(&mut self, folder: &str, cx: &mut Context<Self>) {
        if self.selected_repo.as_deref() != Some(folder) {
            self.select_repo(Some(folder.to_string()), cx);
        }
    }

    /// Closes the bar and clears the row highlight.
    pub fn close_bar(&mut self, cx: &mut Context<Self>) {
        self.selected_repo = None;
        self.selected_tab = BottomBarTab::Overview;
        self.reset_repo_workspace();
        cx.notify();
    }

    fn reset_repo_workspace(&mut self) {
        self.changes = None;
        self.changes_loaded_for = None;
        self.branches.clear();
        self.recent_commits.clear();
        self.history.clear();
        self.unpushed = None;
        self.last_git_error = None;
        self.commit_details = None;
        self.commit_file_patch = None;
        self.github_details = None;
        self.commit_message_draft = None;
        self.readme_draft = None;
        self.wand_error = None;
        self.wand_repo = None;
        self.git_busy = false;
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

    pub fn reproject_pub(&mut self) {
        self.reproject();
    }

    fn reproject(&mut self) {
        self.projected = projection::project(&self.repos, &self.tag_filters, self.sort_mode, self.sort_ascending);
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

    /// The bar header's "Add tag": case-insensitive duplicate check, then the
    /// normal tag toggle (which pushes when absent).
    pub fn add_repo_tag(&mut self, folder: &str, tag: &str, cx: &mut Context<Self>) {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return;
        }
        let duplicate = self
            .repos
            .iter()
            .find(|r| r.folder_path.to_string_lossy() == folder)
            .is_some_and(|r| r.tags.iter().any(|t| t.eq_ignore_ascii_case(trimmed)));
        if duplicate {
            self.notify_info(format!("Tag \"{trimmed}\" already exists"));
            cx.notify();
            return;
        }
        self.toggle_repo_tag(folder, trimmed, cx);
    }

    /// The bar rail's refresh: re-probe the repo, reload its changes and an
    /// open history view.
    pub fn refresh_panel(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_repo.clone() else { return };
        let path = std::path::PathBuf::from(&folder);
        let _ = self.net.send_blocking(NetCommand::ProbeAll { folders: vec![path.clone()] });
        let _ = self.net.send_blocking(NetCommand::LoadChanges { folder: path.clone() });
        if !self.history.is_empty() {
            let _ = self.net.send_blocking(NetCommand::LoadHistory { folder: path });
        }
        cx.notify();
    }

    /// The bar kebab's "Remove from repositories": drops the repo's folder
    /// from the persisted scan roots (disk untouched), rescans so the row
    /// leaves the list, and closes the bar.
    pub fn remove_from_repositories(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.selected_repo.clone() else { return };
        let name = self
            .repos
            .iter()
            .find(|r| r.folder_path.to_string_lossy() == folder)
            .map(|r| r.name.clone())
            .unwrap_or_else(|| folder.clone());
        self.update_settings(|s| {
            if let Some(roots) = &mut s.repos.repo_scan_folders {
                roots.retain(|f| !devtools_core::paths::same_folder_text(f, &folder));
            }
        });
        self.notify_success(format!("Removed \"{name}\" from repositories"));
        self.close_bar(cx);
        self.kick_scan(&self.settings());
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
    /// hits across every tracked repo's notes root (6 repos, 12 notes,
    /// 40 files scanned per repo).
    pub fn run_global_search(&mut self, term: &str, cx: &mut Context<Self>) {
        let term = term.trim();
        if term.is_empty() {
            self.global_search.clear();
            cx.notify();
            return;
        }
        const REPO_CAP: usize = 6;
        const NOTE_CAP: usize = 12;
        let mut hits: Vec<GlobalSearchHit> = self
            .repos
            .iter()
            .filter(|repo| {
                repo.name.to_lowercase().contains(&term.to_lowercase())
                    || repo.folder_path.to_string_lossy().to_lowercase().contains(&term.to_lowercase())
            })
            .take(REPO_CAP)
            .map(|repo| GlobalSearchHit::Repo {
                name: repo.name.clone(),
                folder: repo.folder_path.to_string_lossy().to_string(),
            })
            .collect();

        // note content: the settings' store root, per-repo subfolders
        let settings = self.settings();
        let root = devtools_core::notes::resolve_store_root(&settings.general.notes_store_path);
        for repo in &self.repos {
            if hits.len() >= REPO_CAP + NOTE_CAP {
                break;
            }
            let notes_root = devtools_core::notes::repo_notes_root(&root, &repo.name);
            for hit in devtools_core::notes::search_capped(&notes_root, term, 40) {
                hits.push(GlobalSearchHit::Note {
                    title: hit.title,
                    excerpt: hit.excerpt,
                    path: hit.path,
                });
                if hits.len() >= REPO_CAP + NOTE_CAP {
                    break;
                }
            }
        }
        self.global_search = hits;
        cx.notify();
    }
}

/// Capitalizes a git action label for toast texts ("stage all" → "Stage all").
fn capitalized_label(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A toast's severity: which glyph + color it shows and how long it stays up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    Success,
    Info,
    Error,
}

impl NoticeKind {
    /// Errors read longer; confirmations clear in the classic few seconds.
    pub fn ttl(self) -> std::time::Duration {
        match self {
            NoticeKind::Error => std::time::Duration::from_secs(8),
            NoticeKind::Info | NoticeKind::Success => std::time::Duration::from_secs(4),
        }
    }

    /// The copy helpers answer fixed static strings ("Password copied…",
    /// "No password stored", "Clipboard copy failed…"); classify those.
    pub fn for_copy_message(message: &str) -> Self {
        if message.starts_with("Clipboard copy failed") {
            NoticeKind::Error
        } else if message == "No password stored" {
            NoticeKind::Info
        } else {
            NoticeKind::Success
        }
    }
}

/// One toast in the window's corner stack.
#[derive(Clone, Debug)]
pub struct Notice {
    pub id: u64,
    pub kind: NoticeKind,
    pub text: String,
    born: std::time::Instant,
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

    /// Case-insensitive label lookup; unknown names fall back to Overview.
    pub fn parse(text: &str) -> Self {
        [
            BottomBarTab::Overview,
            BottomBarTab::Changes,
            BottomBarTab::PullRequests,
            BottomBarTab::Issues,
            BottomBarTab::Azure,
        ]
        .into_iter()
        .find(|tab| tab.label().eq_ignore_ascii_case(text.trim()))
        .unwrap_or(BottomBarTab::Overview)
    }
}
