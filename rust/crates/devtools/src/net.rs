//! The net thread: a dedicated OS thread with its own tokio runtime doing all
//! the blocking work (repo scans, git status passes) off the UI. Two unbounded
//! async_channels bridge the sides — commands in, events out. The net thread
//! keeps no repo state; commands carry their own inputs (the store snapshots
//! what a pass needs), so results can never race a stale view of the list.

use async_channel::{Receiver, Sender};
use devtools_core::repo_store::{self, RepoEntry, StatusProbe};
use devtools_core::scan;
use devtools_core::settings::ReposSettings;
use std::path::{Path, PathBuf};

pub enum NetCommand {
    /// The listing scan: direct children of each scan root, user tags and
    /// last-known status carried over from `previous`, cache saved after.
    Scan { settings: ReposSettings, previous: Vec<RepoEntry> },
    /// One status pass over every folder, 4-way parallel, burst-applied.
    ProbeAll { folders: Vec<PathBuf> },
    /// The Add-Repositories dialog's deep scan under one root.
    DeepScan { root: String, depth: i32, settings: ReposSettings },
    /// The Clone-from-URL dialog's clone into a parent directory.
    Clone { url: String, parent_dir: String, name: String },
    /// The Changes tab's shared load: status snapshot + branches + history in
    /// one pass, settled together so the tab paints complete.
    LoadChanges { folder: PathBuf },
    /// One mutation; the net thread reloads the repo's changes and probes its
    /// status after it settles, success or failure.
    Git { folder: PathBuf, action: GitAction },
    /// One change row's expandable patch.
    GetChangePatch { folder: PathBuf, path: String, staged: bool },
    /// The history list's deeper commit cut plus the unpushed set.
    LoadHistory { folder: PathBuf },
    /// One commit's drill-down: files with counts + message body.
    GetCommitDetails { folder: PathBuf, hash: String },
    /// One drill-down file's patch.
    GetCommitFilePatch { folder: PathBuf, hash: String, path: String },
    /// The GitHub activity pass over every repo (2-way parallel gh spawns).
    GitHubPass { folders: Vec<PathBuf>, gh_exe: String },
    /// The Overview sidebar's repo details.
    GitHubDetails { folder: PathBuf, gh_exe: String },
    /// The Azure DevOps activity pass (2-way parallel REST probes).
    AzurePass { folders: Vec<PathBuf>, pat: String, custom_base: String },
}

/// Every state-changing git operation the bottom bar drives. The label rides
/// the result for surfacing.
#[derive(Clone, Debug)]
pub enum GitAction {
    StageAll,
    UnstageAll,
    Stage(String),
    Unstage(String),
    DiscardStaged(Vec<String>),
    DiscardUnstaged(Vec<String>),
    DiscardFile { path: String, is_staged: bool },
    Commit(String),
    Checkout(String),
    CreateBranch { name: String, start: Option<String>, checkout: bool },
    Fetch,
    Pull,
    Push,
    Publish { branch: String },
}

impl GitAction {
    pub fn label(&self) -> &'static str {
        match self {
            GitAction::StageAll => "stage all",
            GitAction::UnstageAll => "unstage all",
            GitAction::Stage(_) => "stage",
            GitAction::Unstage(_) => "unstage",
            GitAction::DiscardStaged(_) => "discard staged",
            GitAction::DiscardUnstaged(_) => "discard unstaged",
            GitAction::DiscardFile { .. } => "discard",
            GitAction::Commit(_) => "commit",
            GitAction::Checkout(_) => "checkout",
            GitAction::CreateBranch { .. } => "create branch",
            GitAction::Fetch => "fetch",
            GitAction::Pull => "pull",
            GitAction::Push => "push",
            GitAction::Publish { .. } => "publish",
        }
    }
}

pub enum UiEvent {
    ScanSettled { repos: Vec<RepoEntry>, removed: Vec<String> },
    StatusSettled { probes: Vec<StatusProbe> },
    DeepScanSettled { root: String, found: Vec<devtools_core::scan::ScannedRepo> },
    CloneSettled { url: String, parent_dir: String, name: String, result: devtools_core::model::GitSyncResult },
    ChangesLoaded {
        folder: String,
        snapshot: Option<devtools_core::model::GitChangeSnapshot>,
        branches: Vec<devtools_core::model::GitBranchRef>,
        recent: Vec<devtools_core::model::GitCommitInfo>,
        unpushed: Option<std::collections::HashSet<String>>,
    },
    GitDone {
        folder: String,
        label: &'static str,
        error: Option<String>,
        /// A commit's short hash on success.
        commit_hash: Option<String>,
    },
    ChangePatchLoaded { folder: String, path: String, staged: bool, patch: String },
    HistoryLoaded {
        folder: String,
        commits: Vec<devtools_core::model::GitCommitInfo>,
        unpushed: Option<std::collections::HashSet<String>>,
    },
    CommitDetailsLoaded { folder: String, details: Option<devtools_core::model::GitCommitDetails> },
    CommitFilePatchLoaded { folder: String, hash: String, path: String, patch: String },
    GitHubSettled { items: Vec<(String, devtools_core::github::GitHubSnapshot)> },
    GitHubDetailsLoaded { folder: String, details: Option<devtools_core::github::GitHubRepoDetails> },
    AzureSettled { items: Vec<(String, devtools_core::azure::AzureSnapshot)> },
}

pub fn spawn_net_thread() -> (std::thread::JoinHandle<()>, Sender<NetCommand>, Receiver<UiEvent>) {
    let (cmd_tx, cmd_rx) = async_channel::unbounded::<NetCommand>();
    let (evt_tx, evt_rx) = async_channel::unbounded::<UiEvent>();
    let handle = std::thread::Builder::new()
        .name("net".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("net runtime");
            rt.block_on(net_main(cmd_rx, evt_tx));
        })
        .expect("net thread");
    (handle, cmd_tx, evt_rx)
}

async fn net_main(cmd_rx: Receiver<NetCommand>, evt_tx: Sender<UiEvent>) {
    while let Ok(cmd) = cmd_rx.recv().await {
        match cmd {
            NetCommand::Scan { settings, previous } => {
                let scanned = tokio::task::spawn_blocking(move || scan::scan(&settings, None))
                    .await
                    .unwrap_or_default();
                let (merged, removed) = repo_store::merge_scan(&previous, &scanned.repos);
                if let Err(err) = repo_store::save_cache(&repo_store::cache_path(), &merged) {
                    tracing::warn!("could not save repo cache: {err}");
                }
                let _ = evt_tx.send_blocking(UiEvent::ScanSettled { repos: merged, removed });
            }
            NetCommand::ProbeAll { folders } => {
                let probes = probe_pass(folders).await;
                let _ = evt_tx.send_blocking(UiEvent::StatusSettled { probes });
            }
            NetCommand::DeepScan { root, depth, settings } => {
                let found = tokio::task::spawn_blocking(move || scan::scan(&settings, Some(depth)))
                    .await
                    .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::DeepScanSettled { root, found: found.repos });
            }
            NetCommand::Clone { url, parent_dir, name } => {
                let result = devtools_core::git::cli::clone(&url, Path::new(&parent_dir), &name).await;
                let _ = evt_tx.send_blocking(UiEvent::CloneSettled { url, parent_dir, name, result });
            }
            NetCommand::GetChangePatch { folder, path, staged } => {
                let patch = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    let path = path.clone();
                    move || devtools_core::git::read::change_file_patch(&folder, &path, staged)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::ChangePatchLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    path,
                    staged,
                    patch,
                });
            }
            NetCommand::GitHubPass { folders, gh_exe } => {
                use std::sync::{Arc, Mutex};
                let collected = Arc::new(Mutex::new(Vec::new()));
                let sink = collected.clone();
                let gh = gh_exe.clone();
                devtools_core::refresh::throttled_pass(folders, 2, move |folder| {
                    let sink = sink.clone();
                    let gh = gh.clone();
                    async move {
                        let snapshot = devtools_core::github::refresh(&folder, &gh).await;
                        sink.lock().unwrap().push((folder.to_string_lossy().to_string(), snapshot));
                    }
                })
                .await;
                let items = Arc::try_unwrap(collected)
                    .map(|m| m.into_inner().unwrap())
                    .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::GitHubSettled { items });
            }
            NetCommand::AzurePass { folders, pat, custom_base } => {
                use std::sync::{Arc, Mutex};
                let collected = Arc::new(Mutex::new(Vec::new()));
                let sink = collected.clone();
                devtools_core::refresh::throttled_pass(folders, 2, move |folder| {
                    let sink = sink.clone();
                    let pat = pat.clone();
                    let custom_base = custom_base.clone();
                    async move {
                        let snapshot = devtools_core::azure::refresh(&folder, &pat, &custom_base).await;
                        sink.lock().unwrap().push((folder.to_string_lossy().to_string(), snapshot));
                    }
                })
                .await;
                let items = Arc::try_unwrap(collected)
                    .map(|m| m.into_inner().unwrap())
                    .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::AzureSettled { items });
            }
            NetCommand::GitHubDetails { folder, gh_exe } => {
                let details = devtools_core::github::repo_details(&folder, &gh_exe).await;
                let _ = evt_tx.send_blocking(UiEvent::GitHubDetailsLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    details,
                });
            }
            NetCommand::LoadHistory { folder } => {
                let (commits, unpushed) = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    move || {
                        (
                            devtools_core::git::read::history_commits(&folder, 200).unwrap_or_default(),
                            devtools_core::git::read::unpushed_hashes(&folder),
                        )
                    }
                })
                .await
                .unwrap_or((Vec::new(), None));
                let _ = evt_tx.send_blocking(UiEvent::HistoryLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    commits,
                    unpushed,
                });
            }
            NetCommand::GetCommitDetails { folder, hash } => {
                let details = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    let hash = hash.clone();
                    move || devtools_core::git::read::commit_details(&folder, &hash)
                })
                .await
                .ok()
                .flatten();
                let _ = evt_tx.send_blocking(UiEvent::CommitDetailsLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    details,
                });
            }
            NetCommand::GetCommitFilePatch { folder, hash, path } => {
                let patch = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    let hash = hash.clone();
                    let path = path.clone();
                    move || devtools_core::git::read::commit_file_patch(&folder, &hash, &path)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                let _ = evt_tx.send_blocking(UiEvent::CommitFilePatchLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    hash,
                    path,
                    patch,
                });
            }
            NetCommand::LoadChanges { folder } => {
                let (snapshot, branches, recent, unpushed) = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    move || {
                        (
                            devtools_core::git::read::change_snapshot(&folder),
                            devtools_core::git::read::branches(&folder).unwrap_or_default(),
                            devtools_core::git::read::recent_commits(&folder).unwrap_or_default(),
                            devtools_core::git::read::unpushed_hashes(&folder),
                        )
                    }
                })
                .await
                .unwrap_or((None, Vec::new(), Vec::new(), None));
                let _ = evt_tx.send_blocking(UiEvent::ChangesLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    snapshot,
                    branches,
                    recent,
                    unpushed,
                });
            }
            NetCommand::Git { folder, action } => {
                let (ok, error, commit_hash) = run_git_action(&folder, &action).await;
                let _ = evt_tx.send_blocking(UiEvent::GitDone {
                    folder: folder.to_string_lossy().to_string(),
                    label: action.label(),
                    error: if ok { None } else { error },
                    commit_hash,
                });
                // every mutation re-checks the repo it touched: the fresh
                // status and the rebuilt changes snapshot land together
                let (snapshot, branches, recent, unpushed) = tokio::task::spawn_blocking({
                    let folder = folder.clone();
                    move || {
                        (
                            devtools_core::git::read::change_snapshot(&folder),
                            devtools_core::git::read::branches(&folder).unwrap_or_default(),
                            devtools_core::git::read::recent_commits(&folder).unwrap_or_default(),
                            devtools_core::git::read::unpushed_hashes(&folder),
                        )
                    }
                })
                .await
                .unwrap_or((None, Vec::new(), Vec::new(), None));
                let _ = evt_tx.send_blocking(UiEvent::ChangesLoaded {
                    folder: folder.to_string_lossy().to_string(),
                    snapshot,
                    branches,
                    recent,
                    unpushed,
                });
                let _ = evt_tx.send_blocking(UiEvent::StatusSettled {
                    probes: probe_pass(vec![folder]).await,
                });
            }
        }
    }
}

async fn run_git_action(folder: &Path, action: &GitAction) -> (bool, Option<String>, Option<String>) {
    use devtools_core::git::cli;
    match action {
        GitAction::StageAll => (cli::stage_all(folder).await.ok, None, None),
        GitAction::UnstageAll => (cli::unstage_all(folder).await.ok, None, None),
        GitAction::Stage(path) => (cli::stage(folder, path).await.ok, None, None),
        GitAction::Unstage(path) => (cli::unstage(folder, path).await.ok, None, None),
        GitAction::DiscardStaged(paths) => (cli::discard_paths(folder, paths, true).await, None, None),
        GitAction::DiscardUnstaged(paths) => (cli::discard_paths(folder, paths, false).await, None, None),
        GitAction::DiscardFile { path, is_staged } => {
            (cli::discard_file(folder, path, *is_staged).await, None, None)
        }
        GitAction::Commit(message) => {
            match cli::commit(folder, message).await {
                Some(hash) => (true, None, Some(hash)),
                None => (false, Some("commit failed".into()), None),
            }
        }
        GitAction::Checkout(branch) => {
            let outcome = cli::checkout(folder, branch).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::CreateBranch { name, start, checkout } => {
            let result = cli::create_branch(folder, name, start.as_deref(), *checkout).await;
            (result.success, result.error, None)
        }
        GitAction::Fetch => {
            let outcome = cli::fetch(folder).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::Pull => {
            let outcome = cli::pull(folder).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::Push => {
            let outcome = cli::push(folder).await;
            (outcome.ok, outcome.error, None)
        }
        GitAction::Publish { branch } => {
            let result = cli::publish_branch(folder, branch).await;
            (result.success, result.error, None)
        }
    }
}

/// One throttled status pass: git2 probes under a 4-way semaphore (parity with
/// the C# service's MaxParallelism), every failure settling as a zeroed
/// snapshot for its folder. Collected, then returned for one burst apply.
async fn probe_pass(folders: Vec<PathBuf>) -> Vec<StatusProbe> {
    use std::sync::{Arc, Mutex};
    let collected = Arc::new(Mutex::new(Vec::with_capacity(folders.len())));
    let sink = collected.clone();
    devtools_core::refresh::throttled_pass(folders, 4, move |folder| {
        let collected = sink.clone();
        async move {
            let fetch_seed = repo_store::read_last_fetch_seed(&folder);
            let probe = devtools_core::git::read::probe(&folder)
                .map(|p| StatusProbe {
                    folder_path: folder.to_string_lossy().to_string(),
                    branch_name: p.branch_name,
                    modified_count: p.modified_count,
                    ahead_count: p.ahead_count,
                    behind_count: p.behind_count,
                    last_commit_at: p.last_commit_at,
                    last_fetch_at: fetch_seed,
                    has_remote: p.has_remote,
                    branch_on_remote: p.branch_on_remote,
                })
                .unwrap_or_else(|| StatusProbe {
                    folder_path: folder.to_string_lossy().to_string(),
                    ..Default::default()
                });
            collected.lock().unwrap().push(probe);
        }
    })
    .await;
    let mut probes = Arc::try_unwrap(collected).map(|m| m.into_inner().unwrap()).unwrap_or_default();
    // keep the table's burst-apply order stable (by folder path)
    probes.sort_by(|a, b| a.folder_path.cmp(&b.folder_path));
    probes
}
