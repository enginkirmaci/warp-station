//! The Worktrees tab's data and its one door call, without a window: a real
//! repository with a real worktree, the net thread's listing, and a stand-in
//! warp-agent door that records the command it receives.

use gpui::prelude::*;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use warp_station::net::{NetCommand, UiEvent, spawn_net_thread};
use warp_station::store::Store;
use warp_station_core::repo_store::RepoEntry;
use warpui_kit_platform::door;

/// One scratch home per binary: `Store` and the door client both read paths
/// under it, and nothing may reach the developer's real `~/.warp`.
fn scratch_home() {
    static HOME: std::sync::Once = std::sync::Once::new();
    HOME.call_once(|| {
        let home = std::env::temp_dir().join(format!(
            "warp-station-worktrees-probe-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(home.join(".warp/agent")).expect("scratch home");
        // SAFETY: process-global env; every test here calls this first.
        unsafe {
            std::env::set_var("HOME", &home);
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", &home);
        };
    });
}

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        status.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

/// A repository at `root` with one commit and one worktree on `agent/one`.
fn repository_with_worktree(root: &Path) -> PathBuf {
    std::fs::create_dir_all(root).expect("repo folder");
    git(root, &["init", "--initial-branch=main", "--quiet"]);
    git(root, &["config", "user.email", "probe@example.com"]);
    git(root, &["config", "user.name", "Probe"]);
    std::fs::write(root.join("readme.md"), "probe\n").expect("readme");
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "first"]);
    git(
        root,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "agent/one",
            ".worktrees/agent-one",
        ],
    );
    root.join(".worktrees").join("agent-one")
}

/// Waits for the next event of one kind, or panics on the deadline.
fn wait_for<T>(
    evt_rx: &async_channel::Receiver<UiEvent>,
    what: &str,
    mut take: impl FnMut(UiEvent) -> Option<T>,
) -> T {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        match evt_rx.recv_blocking() {
            Ok(event) => {
                if let Some(value) = take(event) {
                    return value;
                }
            }
            Err(_) => panic!("the net thread stopped while waiting for {what}"),
        }
    }
}

#[test]
fn the_listing_reads_a_real_worktree() {
    scratch_home();
    let root = std::env::temp_dir().join(format!(
        "warp-station-worktrees-repo-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let worktree = repository_with_worktree(&root);

    let (_handle, cmd_tx, evt_rx) = spawn_net_thread();
    cmd_tx
        .send_blocking(NetCommand::Worktrees {
            folder: root.clone(),
        })
        .expect("send");

    let rows = wait_for(&evt_rx, "the worktree listing", |event| match event {
        UiEvent::WorktreesLoaded { result, .. } => Some(result),
        _ => None,
    })
    .expect("the listing succeeds");
    assert_eq!(rows.len(), 1, "the repository's own checkout is not a row");
    assert_eq!(rows[0].branch, "agent/one");
    assert_eq!(
        rows[0].path,
        worktree.to_string_lossy().to_string(),
        "the row names the checkout's own folder"
    );
    assert_eq!(rows[0].dirty, 0, "a fresh worktree is clean");

    // a change in that checkout shows up on its own row
    std::fs::write(worktree.join("readme.md"), "probe\nedited\n").expect("edit");
    cmd_tx
        .send_blocking(NetCommand::Worktrees {
            folder: root.clone(),
        })
        .expect("send");
    let rows = wait_for(&evt_rx, "the second listing", |event| match event {
        UiEvent::WorktreesLoaded { result, .. } => Some(result),
        _ => None,
    })
    .expect("the listing succeeds");
    assert_eq!(rows[0].dirty, 1);
}

/// A stand-in warp-agent door: the family handshake, one answer, and a log of
/// the frames it received.
struct FakeDoor {
    log: Arc<Mutex<Vec<serde_json::Value>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

impl FakeDoor {
    fn start() -> Self {
        let dir = warpui_kit_platform::paths::app_dir("agent");
        std::fs::create_dir_all(&dir).expect("door dir");
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("addr").port();
        let token = door::random_token();
        door::write_private(&door::token_file_in(&dir), &token).expect("token");
        door::write_private(&door::port_file_in(&dir), &format!("{port}\n")).expect("port");

        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let server = {
            let log = log.clone();
            let stop = stop.clone();
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => serve(stream, &token, &log),
                        Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(2));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Self {
            log,
            stop,
            server: Some(server),
        }
    }

    fn frames(&self) -> Vec<serde_json::Value> {
        self.log.lock().expect("log").clone()
    }
}

impl Drop for FakeDoor {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

fn serve(stream: std::net::TcpStream, token: &str, log: &Mutex<Vec<serde_json::Value>>) {
    let mut writer = match stream.try_clone() {
        Ok(writer) => writer,
        Err(_) => return,
    };
    let mut reader = BufReader::new(stream);
    let mut auth = String::new();
    if reader.read_line(&mut auth).is_err() {
        return;
    }
    let provided = serde_json::from_str::<serde_json::Value>(auth.trim())
        .ok()
        .and_then(|value| {
            value
                .get("auth")
                .and_then(|auth| auth.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    if !door::token_matches(&provided, token) {
        let _ = writer.write_all(b"{\"ok\":false,\"error\":\"auth\"}\n");
        return;
    }
    if writer.write_all(b"{\"ok\":true}\n").is_err() {
        return;
    }
    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let Ok(request) = serde_json::from_str::<serde_json::Value>(request.trim()) else {
        return;
    };
    let id = request.get("id").and_then(|id| id.as_u64()).unwrap_or(1);
    log.lock().expect("log").push(request.clone());
    let frame = door::answer_frame(id, Ok(serde_json::json!({})));
    let _ = writer.write_all(format!("{frame}\n").as_bytes());
}

#[test]
fn the_chat_button_sends_new_chat_for_the_worktree() {
    scratch_home();
    let door = FakeDoor::start();
    let folder = std::env::temp_dir()
        .join(format!(
            "warp-station-worktrees-chat-{}",
            std::process::id()
        ))
        .join(".worktrees")
        .join("agent-one");
    std::fs::create_dir_all(&folder).expect("checkout folder");
    let folder = folder.to_string_lossy().to_string();

    let (_handle, cmd_tx, evt_rx) = spawn_net_thread();
    cmd_tx
        .send_blocking(NetCommand::WorktreeNewChat {
            folder: folder.clone(),
            warp_exe: std::env::current_exe()
                .expect("probe exe")
                .to_string_lossy()
                .to_string(),
        })
        .expect("send");

    // the door answers, and the settled event says it worked
    let error = wait_for(&evt_rx, "the door answer", |event| match event {
        UiEvent::WarpChatStarted { error, .. } => Some(error),
        _ => None,
    });
    assert_eq!(error, None, "a stand-in door answers ok");

    // the status watcher polls the same door, so the chat is found by its
    // command rather than by position
    let frames = door.frames();
    let chat = frames
        .iter()
        .find(|frame| frame.get("cmd").and_then(|cmd| cmd.as_str()) == Some("new-chat"))
        .unwrap_or_else(|| panic!("no new-chat frame among {frames:?}"));
    assert_eq!(
        chat.get("workspace").and_then(|ws| ws.as_str()),
        Some(folder.as_str()),
        "the chat names the checkout as its workspace"
    );
}

/// The tab itself: the strip carries it, and its page draws the row.
#[gpui_kit::gpui::test]
fn the_worktrees_tab_draws_its_rows(cx: &mut gpui::TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("repo", "/probe/repo")]);
        store.select_repo("/probe/repo", cx);
        store
    });
    store.update(cx, |store, cx| {
        store.open_detail_tab(warp_station::store::WorkspaceTab::Worktrees, cx);
        store.on_net_event(
            UiEvent::WorktreesLoaded {
                folder: "/probe/repo".into(),
                result: Ok(vec![warp_station_core::model::WorktreeRow {
                    path: "/probe/repo/.worktrees/agent-one".into(),
                    branch: "agent/one".into(),
                    dirty: 2,
                    ahead: 1,
                    behind: 0,
                }]),
            },
            cx,
        );
    });

    let (_window, cx) = cx.add_window_view(|window, cx| {
        warp_station::ui::window::MainWindow::new(store.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("detail-tab-worktrees").is_some(),
        "the strip carries the Worktrees tab"
    );
    let page = cx
        .debug_bounds("worktrees-page")
        .expect("the tab's page draws");
    let row = cx
        .debug_bounds("worktree-row")
        .expect("the worktree row draws");
    assert!(
        row.origin.y >= page.origin.y,
        "the row sits inside the page: page {page:?}, row {row:?}"
    );
}

/// A repository row for the store: only the tab's selection needs it.
fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
        git_modified_count: 0,
        git_to_push_count: 0,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: true,
        git_branch_published: true,
        git_status_loaded: true,
        github_pr_count: 0,
        github_issue_count: 0,
        azure_pull_count: 0,
        github_web_url: None,
        azure_work_item_count: 0,
    }
}

/// Selecting a repository asks for its worktrees before any tab is opened, so
/// the tab can be drawn from a settled listing. A second ask while the first
/// is still in flight is dropped instead of queueing git again.
#[gpui_kit::gpui::test]
fn selecting_a_repo_asks_for_its_worktrees(cx: &mut gpui::TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("repo", "/probe/repo")]);
        store.select_repo("/probe/repo", cx);
        store
    });

    let take_asks = |net_rx: &async_channel::Receiver<NetCommand>| {
        let mut asks = Vec::new();
        while let Ok(command) = net_rx.try_recv() {
            if let NetCommand::Worktrees { folder } = command {
                asks.push(folder);
            }
        }
        asks
    };

    assert_eq!(
        take_asks(&net_rx),
        vec![PathBuf::from("/probe/repo")],
        "the selection asks for the repository's worktrees with no tab open"
    );

    store.update(cx, |store, cx| store.load_worktrees(cx));
    assert!(
        take_asks(&net_rx).is_empty(),
        "a re-read while the first ask is in flight does not queue a duplicate"
    );
}

/// Refresh asks for the selected repository's worktrees even while another tab
/// is showing, and the worktree read never holds the button's busy state open.
#[gpui_kit::gpui::test]
fn refresh_asks_for_worktrees_with_another_tab_showing(cx: &mut gpui::TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        // the task's "settle the changes answer" assertion is about the
        // worktree read, so the activity passes stay out of `pending`
        store.update_settings(|settings| {
            settings.repos.enable_github = false;
            settings.repos.enable_azure_devops = false;
        });
        store.replace_repos(vec![entry("repo", "/probe/repo")]);
        store.select_repo("/probe/repo", cx);
        store
    });

    // the selection's own ask answers, so the refresh below is a fresh ask;
    // Changes is the resting tab throughout
    store.update(cx, |store, cx| {
        store.on_net_event(
            UiEvent::WorktreesLoaded {
                folder: "/probe/repo".into(),
                result: Ok(Vec::new()),
            },
            cx,
        );
        store.open_detail_tab(warp_station::store::WorkspaceTab::Changes, cx);
    });
    while net_rx.try_recv().is_ok() {}

    store.update(cx, |store, cx| store.refresh_panel(cx));
    let mut commands = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        commands.push(command);
    }
    assert!(
        commands.iter().any(|command| matches!(
            command,
            NetCommand::Worktrees { folder } if folder == Path::new("/probe/repo")
        )),
        "Refresh re-checks the worktrees while Changes is on screen"
    );
    assert!(
        store.read_with(cx, |store, _| store.panel_refreshing()),
        "the refresh starts the button's busy state"
    );

    // only the changes read settles; the worktree answer never arrives
    store.update(cx, |store, cx| {
        store.on_net_event(
            UiEvent::ChangesLoaded {
                folder: "/probe/repo".into(),
                snapshot: None,
                branches: Vec::new(),
                recent: Vec::new(),
                unpushed: None,
            },
            cx,
        );
    });
    assert!(
        !store.read_with(cx, |store, _| store.panel_refreshing()),
        "the worktree read is not part of the button's pending count"
    );
}

fn remove(
    cmd_tx: &async_channel::Sender<NetCommand>,
    evt_rx: &async_channel::Receiver<UiEvent>,
    repo: &Path,
    path: &Path,
    force: bool,
) -> Result<(), String> {
    cmd_tx
        .send_blocking(NetCommand::WorktreeRemove {
            folder: repo.to_path_buf(),
            path: path.to_string_lossy().to_string(),
            force,
        })
        .expect("send");
    wait_for(evt_rx, "the removal", |event| match event {
        UiEvent::WorktreeRemoved { result, .. } => Some(result),
        _ => None,
    })
}

#[test]
fn a_clean_worktree_goes_and_its_branch_stays() {
    scratch_home();
    let root = std::env::temp_dir().join(format!(
        "warp-station-worktrees-remove-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let worktree = repository_with_worktree(&root);
    let (_handle, cmd_tx, evt_rx) = spawn_net_thread();

    remove(&cmd_tx, &evt_rx, &root, &worktree, false).expect("a clean worktree goes");
    assert!(!worktree.exists(), "the checkout's folder is gone");
    // the branch survives its checkout: the work is not thrown away
    assert!(
        std::process::Command::new("git")
            .args(["rev-parse", "--verify", "refs/heads/agent/one"])
            .current_dir(&root)
            .output()
            .expect("git runs")
            .status
            .success(),
        "the branch outlives the worktree"
    );
}

#[test]
fn a_dirty_worktree_refuses_until_forced() {
    scratch_home();
    let root = std::env::temp_dir().join(format!(
        "warp-station-worktrees-dirty-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let worktree = repository_with_worktree(&root);
    std::fs::write(worktree.join("readme.md"), "uncommitted\n").expect("edit");
    let (_handle, cmd_tx, evt_rx) = spawn_net_thread();

    assert!(
        remove(&cmd_tx, &evt_rx, &root, &worktree, false).is_err(),
        "uncommitted work refuses the plain removal"
    );
    assert!(worktree.exists(), "nothing was deleted");

    remove(&cmd_tx, &evt_rx, &root, &worktree, true).expect("the force confirm removes it");
    assert!(!worktree.exists());
}

#[test]
fn prune_clears_a_worktree_whose_folder_is_gone() {
    scratch_home();
    let root = std::env::temp_dir().join(format!(
        "warp-station-worktrees-prune-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let worktree = repository_with_worktree(&root);
    std::fs::remove_dir_all(&worktree).expect("delete the checkout");
    let (_handle, cmd_tx, evt_rx) = spawn_net_thread();

    cmd_tx
        .send_blocking(NetCommand::WorktreePrune {
            folder: root.clone(),
        })
        .expect("send");
    let pruned = wait_for(&evt_rx, "the prune", |event| match event {
        UiEvent::WorktreesPruned { result, .. } => Some(result),
        _ => None,
    })
    .expect("the prune succeeds");
    assert_eq!(pruned, 1, "the stale entry went");

    // and the tab's next read agrees
    cmd_tx
        .send_blocking(NetCommand::Worktrees {
            folder: root.clone(),
        })
        .expect("send");
    let rows = wait_for(&evt_rx, "the listing", |event| match event {
        UiEvent::WorktreesLoaded { result, .. } => Some(result),
        _ => None,
    })
    .expect("the listing succeeds");
    assert!(rows.is_empty(), "no rows are left: {rows:?}");
}

#[gpui_kit::gpui::test]
fn a_refused_removal_draws_its_force_confirm(cx: &mut gpui::TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("repo", "/probe/repo")]);
        store.select_repo("/probe/repo", cx);
        store
    });
    let path = "/probe/repo/.worktrees/agent-one".to_string();
    store.update(cx, |store, cx| {
        store.open_detail_tab(warp_station::store::WorkspaceTab::Worktrees, cx);
        store.on_net_event(
            UiEvent::WorktreesLoaded {
                folder: "/probe/repo".into(),
                result: Ok(vec![warp_station_core::model::WorktreeRow {
                    path: path.clone(),
                    branch: "agent/one".into(),
                    dirty: 1,
                    ahead: 0,
                    behind: 0,
                }]),
            },
            cx,
        );
        store.on_net_event(
            UiEvent::WorktreeRemoved {
                folder: "/probe/repo".into(),
                path: path.clone(),
                force: false,
                result: Err("fatal: contains modified or untracked files".into()),
            },
            cx,
        );
    });

    let (_window, cx) = cx.add_window_view(|window, cx| {
        warp_station::ui::window::MainWindow::new(store.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("worktree-removal-confirm").is_some(),
        "the refused row asks before forcing"
    );
    assert!(
        cx.debug_bounds("worktree-force").is_some(),
        "the confirm carries the force action"
    );
}
