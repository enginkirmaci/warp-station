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
}

pub enum UiEvent {
    ScanSettled { repos: Vec<RepoEntry>, removed: Vec<String> },
    StatusSettled { probes: Vec<StatusProbe> },
    DeepScanSettled { root: String, found: Vec<devtools_core::scan::ScannedRepo> },
    CloneSettled { url: String, parent_dir: String, name: String, result: devtools_core::model::GitSyncResult },
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
