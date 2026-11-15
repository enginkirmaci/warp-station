//! Deep headless probe of the merge surface: the branch menu's Merge verb, the
//! stopped-merge band, conflicted rows, the conflict pane, the merge editor's
//! whole-file panes and direct Result editing, Continue/Abort, the dirty-tree
//! refusal, and the owner's stopped `.gitignore` merge copied into /tmp.
//!
//! Every fixture is a fresh scratch repository under `/tmp/merge-deep-ui-*`.
//! The UI is driven through `MainWindow`; the git work the app's net thread
//! would do is executed here with the real core functions, so the store's
//! event path is exercised end to end.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use gpui::prelude::*;
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext};
use warp_station::net::{GitAction, MergeRun, NetCommand, UiEvent};
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::window::MainWindow;
use warp_station_core::git::cli::{self, GitOutcome, MergeOutcome};
use warp_station_core::git::conflict::{self, ConflictSide};
use warp_station_core::git::read;

static SEQ: AtomicU64 = AtomicU64::new(0);
static HOME_ONCE: std::sync::Once = std::sync::Once::new();

/// Every store in this binary reads the repo cache and shared UI values from
/// `$HOME/.warp`; point HOME at a scratch folder once so no probe reads or
/// writes the developer's live tree. `Once` makes the parallel tests wait for
/// the one redirect instead of racing it.
fn scratch_home() {
    HOME_ONCE.call_once(|| {
        let home = std::env::temp_dir().join(format!("merge-deep-ui-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".warp")).expect("scratch home");
        // SAFETY: process-global env, set once before any store is built and
        // while the other test threads are blocked in `call_once`.
        unsafe {
            std::env::set_var("HOME", &home);
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", &home);
            std::env::set_var("XDG_CONFIG_HOME", home.join(".config"));
        }
    });
}

// ---------------------------------------------------------------------------
// scratch repositories
// ---------------------------------------------------------------------------

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("merge-deep-ui-{name}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let scratch = Self { dir };
        scratch.git(&["init", "-q", "-b", "main"]);
        scratch.git(&["config", "user.name", "t"]);
        scratch.git(&["config", "user.email", "t@example.com"]);
        scratch.git(&["config", "commit.gpgsign", "false"]);
        scratch.git(&["config", "rerere.enabled", "false"]);
        scratch
    }

    fn existing(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn folder(&self) -> String {
        self.dir.to_string_lossy().to_string()
    }

    fn output(&self, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("git runs")
    }

    fn git(&self, args: &[&str]) -> String {
        let out = self.output(args);
        assert!(
            out.status.success(),
            "git {args:?} failed in {}:\nstdout: {}\nstderr: {}",
            self.dir.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        std::fs::write(full, text).expect("fixture file");
    }

    fn write_bytes(&self, path: &str, bytes: &[u8]) {
        std::fs::write(self.dir.join(path), bytes).expect("fixture file");
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.dir.join(path)).expect("fixture file read")
    }

    fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    fn has_merge_head(&self) -> bool {
        self.dir.join(".git/MERGE_HEAD").exists()
    }

    fn status_short(&self) -> String {
        self.git(&["status", "--short"])
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

/// Two branches edit the same line; the fixture is left clean on `main`.
fn divergent_repo(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.write("shared.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("shared.txt", "feat\n");
    repo.commit_all("feat edit");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("shared.txt", "main\n");
    repo.commit_all("main edit");
    repo
}

/// One file with two separated conflict blocks.
fn two_block_repo(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.write(
        "multi.txt",
        "one\nbase1\ntwo\nthree\nfour\nfive\nbase2\nend\n",
    );
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write(
        "multi.txt",
        "one\nfeat1\ntwo\nthree\nfour\nfive\nfeat2\nend\n",
    );
    repo.commit_all("feat edits");
    repo.git(&["checkout", "-q", "main"]);
    repo.write(
        "multi.txt",
        "one\nmain1\ntwo\nthree\nfour\nfive\nmain2\nend\n",
    );
    repo.commit_all("main edits");
    repo
}

/// Two separate conflicted files.
fn two_file_repo(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.write("a.txt", "base a\n");
    repo.write("b.txt", "base b\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("a.txt", "feat a\n");
    repo.write("b.txt", "feat b\n");
    repo.commit_all("feat edits");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("a.txt", "main a\n");
    repo.write("b.txt", "main b\n");
    repo.commit_all("main edits");
    repo
}

/// Git hoists the incoming file's `B` line above the markers, but stage 2
/// keeps `B` on line 2 and the long local addition on line 3. The parsed
/// `local_lines` points at `B`, so the block's word span overruns it.
fn hoisted_prefix_repo(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.write("f.txt", "A\nB\nC\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("f.txt", "B\nC\nZ\n");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("f.txt", "A\nB\nCONFLICT_LONG\n");
    repo.commit_all("main");
    repo
}

fn stop_with_merge(repo: &Scratch, source: &str) {
    let outcome = block_on(cli::merge(repo.path(), source));
    assert_eq!(outcome, MergeOutcome::Stopped, "the fixture must stop");
    assert!(repo.has_merge_head());
}

// ---------------------------------------------------------------------------
// store / window plumbing
// ---------------------------------------------------------------------------

fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-merge-deep-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

fn probe_store(
    cx: &mut TestAppContext,
    repo: &Scratch,
    name: &str,
) -> (gpui::Entity<Store>, async_channel::Receiver<NetCommand>) {
    let folder = repo.folder();
    let snapshot = read::change_snapshot(repo.path());
    let branches = read::branches(repo.path()).unwrap_or_default();
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, scratch_service(name)).without_flow_cache();
        store.replace_repos(vec![warp_station_core::repo_store::RepoEntry {
            name: "merge-repo".into(),
            folder_path: repo.path().to_path_buf(),
            solution_path: None,
            tags: vec![],
            git_branch_name: Some("main".into()),
            git_modified_count: 1,
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: false,
            git_branch_published: false,
            git_status_loaded: true,
            github_pr_count: 0,
            github_issue_count: 0,
            azure_pull_count: 0,
            github_web_url: None,
            azure_work_item_count: 0,
        }]);
        store.select_repo(&folder, cx);
        store.set_changes(snapshot);
        store.branches = branches;
        store.changes_loaded_for = Some(folder.clone());
        store.open_detail_tab(WorkspaceTab::Changes, cx);
        store
    });
    (store, net_rx)
}

/// `add_window_view` returns a context borrowed from `TestAppContext`, so the
/// window setup is a macro rather than a function returning the borrow.
macro_rules! open_window {
    ($test_cx:ident, $store:expr) => {{
        let (window, cx) =
            $test_cx.add_window_view(|window, cx| MainWindow::new($store.clone(), window, cx));
        cx.run_until_parked();
        cx.update(|window, cx| drop(window.draw(cx)));
        (window, cx)
    }};
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be on screen"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

fn right_click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} must be on screen"));
    let at = bounds.center();
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::default());
    draw(cx);
}

fn drain(rx: &async_channel::Receiver<NetCommand>) -> Vec<NetCommand> {
    let mut commands = Vec::new();
    while let Ok(command) = rx.try_recv() {
        commands.push(command);
    }
    commands
}

fn merge_sources(commands: &[NetCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            NetCommand::Git {
                action: GitAction::Merge { source },
                ..
            } => Some(source.clone()),
            _ => None,
        })
        .collect()
}

fn resolve_commands(commands: &[NetCommand]) -> Vec<(String, Option<String>)> {
    commands
        .iter()
        .filter_map(|command| match command {
            NetCommand::Git {
                action: GitAction::ResolveConflict { path, text },
                ..
            } => Some((path.clone(), text.clone())),
            _ => None,
        })
        .collect()
}

fn bulk_commands(commands: &[NetCommand]) -> Vec<(String, ConflictSide)> {
    commands
        .iter()
        .filter_map(|command| match command {
            NetCommand::Git {
                action: GitAction::ResolveConflictBulk { path, side },
                ..
            } => Some((path.clone(), *side)),
            _ => None,
        })
        .collect()
}

fn continue_commands(commands: &[NetCommand]) -> usize {
    commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                NetCommand::Git {
                    action: GitAction::MergeContinue,
                    ..
                }
            )
        })
        .count()
}

fn abort_commands(commands: &[NetCommand]) -> usize {
    commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                NetCommand::Git {
                    action: GitAction::MergeAbort,
                    ..
                }
            )
        })
        .count()
}

fn load_requests(commands: &[NetCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| match command {
            NetCommand::LoadMergeConflict { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect()
}

fn git_done(
    cx: &mut VisualTestContext,
    store: &gpui::Entity<Store>,
    repo: &Scratch,
    label: &'static str,
    merge: Option<MergeRun>,
    error: Option<String>,
) {
    let folder = repo.folder();
    cx.update_entity(store, |store, cx| {
        store.on_net_event(
            UiEvent::GitDone {
                folder,
                label,
                error,
                commit_hash: None,
                detail: None,
                merge,
            },
            cx,
        );
    });
}

fn reload_changes(cx: &mut VisualTestContext, store: &gpui::Entity<Store>, repo: &Scratch) {
    let folder = repo.folder();
    let snapshot = read::change_snapshot(repo.path());
    let branches = read::branches(repo.path()).unwrap_or_default();
    let recent = read::recent_commits(repo.path()).unwrap_or_default();
    let unpushed = read::unpushed_hashes(repo.path());
    cx.update_entity(store, |store, cx| {
        store.on_net_event(
            UiEvent::ChangesLoaded {
                folder,
                snapshot,
                branches,
                recent,
                unpushed,
            },
            cx,
        );
    });
    draw(cx);
}

fn load_conflict(
    cx: &mut VisualTestContext,
    store: &gpui::Entity<Store>,
    repo: &Scratch,
    path: &str,
) {
    let folder = repo.folder();
    let file = conflict::conflict_file(repo.path(), path);
    cx.update_entity(store, |store, cx| {
        store.on_net_event(
            UiEvent::MergeConflictLoaded {
                folder,
                path: path.to_string(),
                file,
            },
            cx,
        );
    });
    draw(cx);
}

fn merge_run(outcome: &MergeOutcome, source: &str) -> MergeRun {
    match outcome {
        MergeOutcome::Merged => MergeRun::Merged {
            source: source.to_string(),
        },
        MergeOutcome::UpToDate => MergeRun::UpToDate {
            source: source.to_string(),
        },
        MergeOutcome::Stopped => MergeRun::Stopped {
            source: source.to_string(),
        },
        MergeOutcome::Failed(error) => MergeRun::Failed {
            action: "Merge",
            error: error.clone(),
        },
    }
}

/// Runs the command the store put on the wire and feeds the app's event back.
fn settle_merge(
    cx: &mut VisualTestContext,
    store: &gpui::Entity<Store>,
    repo: &Scratch,
    source: &str,
) -> MergeOutcome {
    let outcome = block_on(cli::merge(repo.path(), source));
    let run = merge_run(&outcome, source);
    let error = match &outcome {
        MergeOutcome::Failed(error) => error.clone(),
        _ => None,
    };
    git_done(cx, store, repo, "merge", Some(run), error);
    reload_changes(cx, store, repo);
    outcome
}

fn settle_continue(
    cx: &mut VisualTestContext,
    store: &gpui::Entity<Store>,
    repo: &Scratch,
) -> GitOutcome {
    let outcome = block_on(cli::merge_continue(repo.path()));
    let run = if outcome.ok {
        MergeRun::Continued
    } else {
        MergeRun::Failed {
            action: "Continue merge",
            error: outcome.error.clone(),
        }
    };
    git_done(
        cx,
        store,
        repo,
        "merge continue",
        Some(run),
        outcome.error.clone(),
    );
    reload_changes(cx, store, repo);
    outcome
}

fn settle_abort(
    cx: &mut VisualTestContext,
    store: &gpui::Entity<Store>,
    repo: &Scratch,
) -> GitOutcome {
    let outcome = block_on(cli::merge_abort(repo.path()));
    let run = if outcome.ok {
        MergeRun::Aborted
    } else {
        MergeRun::Failed {
            action: "Abort merge",
            error: outcome.error.clone(),
        }
    };
    git_done(
        cx,
        store,
        repo,
        "merge abort",
        Some(run),
        outcome.error.clone(),
    );
    reload_changes(cx, store, repo);
    outcome
}

fn settle_resolve(
    cx: &mut VisualTestContext,
    store: &gpui::Entity<Store>,
    repo: &Scratch,
    path: &str,
    text: Option<&str>,
) -> GitOutcome {
    let outcome = block_on(conflict::resolve(repo.path(), path, text));
    git_done(
        cx,
        store,
        repo,
        "resolve conflict",
        None,
        outcome.error.clone(),
    );
    reload_changes(cx, store, repo);
    outcome
}

fn settle_bulk(
    cx: &mut VisualTestContext,
    store: &gpui::Entity<Store>,
    repo: &Scratch,
    path: &str,
    side: ConflictSide,
) -> GitOutcome {
    let outcome = block_on(conflict::resolve_all(repo.path(), path, side));
    git_done(
        cx,
        store,
        repo,
        "resolve conflict",
        None,
        outcome.error.clone(),
    );
    reload_changes(cx, store, repo);
    outcome
}

fn notices(store: &gpui::Entity<Store>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.read_entity(store, |store, _| {
        store
            .notices
            .list()
            .iter()
            .map(|notice| notice.text.clone())
            .collect()
    })
}

fn merge_conflicted(store: &gpui::Entity<Store>, cx: &mut VisualTestContext) -> Vec<String> {
    cx.read_entity(store, |store, _| {
        store
            .merge_state()
            .map(|merge| merge.conflicted.clone())
            .unwrap_or_default()
    })
}

/// Opens the branch dropdown and right-clicks the `feat` row, leaving the row
/// menu up.
fn open_feature_row_menu(cx: &mut VisualTestContext) {
    click(cx, "branch-button");
    assert!(
        cx.debug_bounds("branch-row-feat").is_some(),
        "the feat row is in the dropdown"
    );
    right_click(cx, "branch-row-feat");
    assert!(
        cx.debug_bounds("branch-menu-catcher").is_some(),
        "the row menu is up"
    );
}

fn close_row_menu(cx: &mut VisualTestContext) {
    if cx.debug_bounds("branch-menu-catcher").is_some() {
        click(cx, "branch-menu-catcher");
    }
}

// ---------------------------------------------------------------------------
// 1. Branch menu: fast-forward then already contained
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn the_branch_menu_merges_a_fast_forward_then_says_up_to_date(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = Scratch::new("menu-ff");
    repo.write("a.txt", "one\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("a.txt", "two\n");
    repo.commit_all("feat edit");
    repo.git(&["checkout", "-q", "main"]);
    let feat = repo.git(&["rev-parse", "refs/heads/feat"]);

    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-menu-ff");
    let (_window, cx) = open_window!(cx, &store);

    // ---- Merge into current branch, from the row's context menu ----
    open_feature_row_menu(cx);
    assert!(
        cx.debug_bounds("branch-menu-merge").is_some(),
        "the row menu offers Merge"
    );
    click(cx, "branch-menu-merge");
    let commands = drain(&net_rx);
    assert_eq!(
        merge_sources(&commands),
        vec!["feat".to_string()],
        "the menu sends the row's branch"
    );
    assert!(
        cx.debug_bounds("merge-band").is_some(),
        "the flight band appears while the merge runs"
    );
    assert!(
        cx.read_entity(&store, |store, _| store.merge_running_source()
            == Some("feat")),
        "the band knows the source while in flight"
    );

    let outcome = settle_merge(cx, &store, &repo, "feat");
    assert_eq!(outcome, MergeOutcome::Merged);
    assert_eq!(repo.head(), feat, "HEAD advanced");
    assert!(
        cx.debug_bounds("merge-band").is_none(),
        "the band is gone after the merge"
    );
    assert!(
        notices(&store, cx)
            .iter()
            .any(|notice| notice == "Merged feat into main"),
        "the success toast names both branches: {:?}",
        notices(&store, cx)
    );
    assert!(
        cx.read_entity(&store, |store, _| store.merge_state().is_none()),
        "no stopped merge state"
    );

    // ---- The same branch again: already contained, no merge commit ----
    let before = repo.head();
    open_feature_row_menu(cx);
    click(cx, "branch-menu-merge");
    let commands = drain(&net_rx);
    assert_eq!(merge_sources(&commands), vec!["feat".to_string()]);
    let outcome = settle_merge(cx, &store, &repo, "feat");
    assert_eq!(outcome, MergeOutcome::UpToDate);
    assert_eq!(repo.head(), before, "up to date does not move HEAD");
    assert!(
        cx.debug_bounds("merge-band").is_none(),
        "up to date leaves no band"
    );
    assert!(
        notices(&store, cx)
            .iter()
            .any(|notice| notice == "feat is already contained in main"),
        "{:?}",
        notices(&store, cx)
    );
}

// ---------------------------------------------------------------------------
// 2. A stop: band, conflicted row, conflict pane, disabled Continue
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn a_stopped_merge_draws_the_band_row_and_conflict_pane(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("stop-surface");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-stop");
    let (_window, cx) = open_window!(cx, &store);

    open_feature_row_menu(cx);
    click(cx, "branch-menu-merge");
    drain(&net_rx);
    let outcome = settle_merge(cx, &store, &repo, "feat");
    assert_eq!(outcome, MergeOutcome::Stopped);

    assert!(
        cx.debug_bounds("merge-band").is_some(),
        "the stopped merge takes the band"
    );
    assert_eq!(merge_conflicted(&store, cx), ["shared.txt"]);
    assert!(
        cx.debug_bounds("u-shared.txt").is_some(),
        "the conflicted row is in the list"
    );
    assert!(
        cx.debug_bounds("open-merge-editor").is_some(),
        "the conflict pane offers the editor"
    );
    assert!(
        notices(&store, cx)
            .iter()
            .any(|notice| notice == "Merge stopped: 1 conflict to resolve"),
        "the stop toast waits for the reload's count: {:?}",
        notices(&store, cx)
    );

    // Continue is on screen but disabled while a conflict remains.
    assert!(cx.debug_bounds("merge-continue").is_some());
    click(cx, "merge-continue");
    assert_eq!(
        continue_commands(&drain(&net_rx)),
        0,
        "a conflicted merge cannot continue"
    );
    assert!(cx.debug_bounds("merge-abort").is_some());

    // The row menu's Merge is visible but disabled while a merge is stopped.
    open_feature_row_menu(cx);
    assert!(cx.debug_bounds("branch-menu-merge").is_some());
    click(cx, "branch-menu-merge");
    assert_eq!(
        merge_sources(&drain(&net_rx)),
        Vec::<String>::new(),
        "Merge is disabled during a stopped merge"
    );
    close_row_menu(cx);
    assert!(cx.debug_bounds("branch-menu-catcher").is_none());

    // Opening the editor without its file shows the loading state and no panic.
    click(cx, "open-merge-editor");
    assert_eq!(load_requests(&drain(&net_rx)), ["shared.txt"]);
    assert!(cx.debug_bounds("merge-editor").is_some());
    assert!(
        cx.debug_bounds("merge-local").is_none(),
        "the panes wait for the conflict read"
    );
    load_conflict(cx, &store, &repo, "shared.txt");
    assert!(cx.debug_bounds("merge-local").is_some());
    assert!(cx.debug_bounds("merge-incoming").is_some());
    assert!(cx.debug_bounds("merge-result-scroll").is_some());
    assert!(cx.debug_bounds("merge-block-0").is_some());
    assert!(cx.debug_bounds("merge-accept").is_some());
    assert!(
        cx.debug_bounds("merge-base-toggle").is_none(),
        "merge style has no base pane"
    );
}

// ---------------------------------------------------------------------------
// 3. Accept (write + stage), then Continue commits with the prepared message
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn taking_local_accepts_then_continue_lands_the_merge_commit(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("accept-continue");
    stop_with_merge(&repo, "feat");
    let before = repo.head();
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-accept");
    let (_window, cx) = open_window!(cx, &store);

    click(cx, "open-merge-editor");
    assert_eq!(load_requests(&drain(&net_rx)), ["shared.txt"]);
    load_conflict(cx, &store, &repo, "shared.txt");

    click(cx, "merge-local-take");
    click(cx, "merge-accept");
    let commands = drain(&net_rx);
    assert_eq!(
        resolve_commands(&commands),
        vec![("shared.txt".to_string(), Some("main\n".to_string()))],
        "Accept sent the local side"
    );
    let resolved = settle_resolve(cx, &store, &repo, "shared.txt", Some("main\n"));
    assert!(resolved.ok, "{:?}", resolved.error);
    assert_eq!(repo.read("shared.txt"), "main\n");
    assert_eq!(repo.git(&["ls-files", "-u"]), "", "Accept staged the file");

    assert!(
        cx.debug_bounds("merge-editor").is_none(),
        "Accept closes the editor"
    );
    assert!(
        cx.debug_bounds("u-shared.txt").is_none(),
        "the reload clears the conflicted row"
    );
    assert!(
        cx.read_entity(&store, |store, _| store
            .merge_state()
            .is_some_and(|merge| merge.conflicted.is_empty())),
        "the band is in its ready phase"
    );
    assert!(cx.debug_bounds("merge-band").is_some());
    click(cx, "merge-continue");
    let commands = drain(&net_rx);
    assert_eq!(continue_commands(&commands), 1);
    let continued = settle_continue(cx, &store, &repo);
    assert!(continued.ok, "{:?}", continued.error);
    assert!(!repo.has_merge_head());
    assert_ne!(repo.head(), before);
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "Merge branch 'feat'"
    );
    assert!(
        cx.debug_bounds("merge-band").is_none(),
        "the band is gone after Continue"
    );
    assert!(
        notices(&store, cx)
            .iter()
            .any(|notice| notice == "Merge committed"),
        "{:?}",
        notices(&store, cx)
    );
}

// ---------------------------------------------------------------------------
// 4. Take Incoming and both-side orders
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn taking_incoming_emits_the_incoming_side(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("incoming");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-incoming");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "shared.txt");
    click(cx, "merge-incoming-take");
    click(cx, "merge-accept");
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![("shared.txt".to_string(), Some("feat\n".to_string()))]
    );
}

#[gpui_kit::gpui::test]
fn take_both_writes_local_then_incoming(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("both-li");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-both-li");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "shared.txt");
    click(cx, "merge-both-local-first");
    assert!(cx.debug_bounds("merge-accept").is_some());
    click(cx, "merge-accept");
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![("shared.txt".to_string(), Some("main\nfeat\n".to_string()))]
    );
}

#[gpui_kit::gpui::test]
fn take_both_writes_incoming_then_local(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("both-il");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-both-il");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "shared.txt");
    click(cx, "merge-both-incoming-first");
    click(cx, "merge-accept");
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![("shared.txt".to_string(), Some("feat\nmain\n".to_string()))]
    );
}

/// The local resolution makes the index equal HEAD, so the Changes list is
/// empty; the band must still say the merge is ready.
#[gpui_kit::gpui::test]
fn a_clean_resolved_merge_still_draws_the_band(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let repo = divergent_repo("clean-resolved-ui");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-clean-resolved");
    let (_window, cx) = open_window!(cx, &store);
    let resolved = settle_resolve(cx, &store, &repo, "shared.txt", Some("main\n"));
    assert!(resolved.ok, "{:?}", resolved.error);
    assert_eq!(repo.status_short(), "");
    assert!(
        cx.debug_bounds("merge-band").is_some(),
        "the band survives the empty Changes list"
    );
    assert!(cx.debug_bounds("merge-continue").is_some());
    click(cx, "merge-continue");
    assert_eq!(continue_commands(&drain(&net_rx)), 1);
}

// ---------------------------------------------------------------------------
// 5. Direct Result editing, Reset, and the marker refusal
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn the_result_can_be_edited_and_reset_restores_the_markers(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("result-edit");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-result");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "shared.txt");

    // The seeded Result still holds markers: Accept must do nothing.
    click(cx, "merge-accept");
    assert!(
        resolve_commands(&drain(&net_rx)).is_empty(),
        "Accept is refused while a marker line remains"
    );

    // Type at the end of the Result: the markers are still there, so the same
    // refusal holds after a user edit.
    click(cx, "merge-result");
    cx.simulate_keystrokes("ctrl-end");
    cx.simulate_input("manual");
    cx.simulate_keystrokes("enter");
    draw(cx);
    click(cx, "merge-accept");
    assert!(
        resolve_commands(&drain(&net_rx)).is_empty(),
        "a hand-edited marker file is still refused"
    );

    // Reset puts the marker-bearing worktree text back.
    click(cx, "merge-reset");
    click(cx, "merge-accept");
    assert!(resolve_commands(&drain(&net_rx)).is_empty());

    // Take Local, then append a line of our own: Accept sends the edited text.
    click(cx, "merge-local-take");
    click(cx, "merge-result");
    cx.simulate_keystrokes("ctrl-end");
    cx.simulate_input("edited");
    cx.simulate_keystrokes("enter");
    draw(cx);
    click(cx, "merge-accept");
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![("shared.txt".to_string(), Some("main\nedited\n".to_string()))],
        "the direct Result edit is what Accept writes"
    );
}

// A CRLF file and a unicode file go through the editor the same way.
#[gpui_kit::gpui::test]
fn crlf_and_unicode_files_edit_without_a_panic(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = Scratch::new("crlf-unicode-ui");
    repo.write("crlf.txt", "a\r\nbase\r\n");
    repo.write("unicode.txt", "şapka\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("crlf.txt", "a\r\nfeat\r\n");
    repo.write("unicode.txt", "şapka ııı 😀\n");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("crlf.txt", "a\r\nmain\r\n");
    repo.write("unicode.txt", "şapka ğğğ 😀\n");
    repo.commit_all("main");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-crlf-unicode");
    let (_window, cx) = open_window!(cx, &store);

    right_click(cx, "u-crlf.txt");
    click(cx, "file-menu-open");
    assert_eq!(load_requests(&drain(&net_rx)), ["crlf.txt"]);
    load_conflict(cx, &store, &repo, "crlf.txt");
    click(cx, "merge-local-take");
    click(cx, "merge-accept");
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![("crlf.txt".to_string(), Some("a\r\nmain\r\n".to_string()))],
        "the CRLF file keeps its carriage returns"
    );
    let resolved = settle_resolve(cx, &store, &repo, "crlf.txt", Some("a\r\nmain\r\n"));
    assert!(resolved.ok, "{:?}", resolved.error);

    right_click(cx, "u-unicode.txt");
    click(cx, "file-menu-open");
    assert_eq!(load_requests(&drain(&net_rx)), ["unicode.txt"]);
    load_conflict(cx, &store, &repo, "unicode.txt");
    click(cx, "merge-incoming-take");
    click(cx, "merge-accept");
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![(
            "unicode.txt".to_string(),
            Some("şapka ııı 😀\n".to_string())
        )],
        "the unicode side is written byte-for-byte"
    );
}

/// Known bug: while the Result owns the keyboard, `ctrl-enter` is bound by
/// the shared `Input` context to `Enter { secondary: true }` (gpui-base
/// `input/base/state.rs:203`), so the merge editor's handler never sees it.
/// Clicking Accept still works. This pin turns red when the shortcut is fixed.
#[gpui_kit::gpui::test]
#[should_panic(expected = "Ctrl+Enter works")]
fn ctrl_enter_is_swallowed_by_the_result_textarea(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let repo = divergent_repo("ctrl-enter-result");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-ctrl-enter-result");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "shared.txt");
    click(cx, "merge-local-take");
    click(cx, "merge-result");
    cx.simulate_keystrokes("ctrl-enter");
    draw(cx);
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![("shared.txt".to_string(), Some("main\n".to_string()))],
        "Ctrl+Enter works while the Result owns the keyboard"
    );
}

#[gpui_kit::gpui::test]
fn escape_closes_the_editor_from_the_result_textarea(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let repo = divergent_repo("escape-result");
    stop_with_merge(&repo, "feat");
    let (store, _net_rx) = probe_store(cx, &repo, "merge-deep-escape-result");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    load_conflict(cx, &store, &repo, "shared.txt");
    click(cx, "merge-result");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("merge-editor").is_none(),
        "Escape closes the editor while the Result owns the keyboard"
    );
}

// ---------------------------------------------------------------------------
// 6. The whole-file row menu and its bulk takes
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn the_conflicted_rows_menu_bulk_resolves_one_file(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = two_file_repo("bulk");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-bulk");
    let (_window, cx) = open_window!(cx, &store);

    assert!(cx.debug_bounds("u-a.txt").is_some());
    assert!(cx.debug_bounds("u-b.txt").is_some());
    right_click(cx, "u-b.txt");
    assert!(cx.debug_bounds("file-menu-catcher").is_some());
    assert!(cx.debug_bounds("file-menu-local").is_some());
    assert!(cx.debug_bounds("file-menu-incoming").is_some());
    click(cx, "file-menu-local");
    let commands = drain(&net_rx);
    assert_eq!(
        bulk_commands(&commands),
        vec![("b.txt".to_string(), ConflictSide::Local)],
        "the row menu's Take Local reaches the net thread"
    );
    let bulk = settle_bulk(cx, &store, &repo, "b.txt", ConflictSide::Local);
    assert!(bulk.ok, "{:?}", bulk.error);
    assert_eq!(repo.read("b.txt"), "main b\n");
    assert!(
        cx.debug_bounds("u-b.txt").is_none(),
        "the bulk take clears b.txt"
    );
    assert!(
        cx.debug_bounds("u-a.txt").is_some(),
        "a.txt is still conflicted"
    );
    assert_eq!(merge_conflicted(&store, cx), ["a.txt"]);

    right_click(cx, "u-a.txt");
    click(cx, "file-menu-incoming");
    let commands = drain(&net_rx);
    assert_eq!(
        bulk_commands(&commands),
        vec![("a.txt".to_string(), ConflictSide::Incoming)]
    );
    let bulk = settle_bulk(cx, &store, &repo, "a.txt", ConflictSide::Incoming);
    assert!(bulk.ok, "{:?}", bulk.error);
    assert_eq!(repo.read("a.txt"), "feat a\n");
    assert!(merge_conflicted(&store, cx).is_empty());
    assert!(cx.debug_bounds("merge-continue").is_some());
}

// ---------------------------------------------------------------------------
// 7. diff3's Base pane and the column layout
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn a_diff3_conflict_offers_the_base_pane(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("diff3-ui");
    repo.git(&["config", "merge.conflictStyle", "diff3"]);
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-diff3");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "shared.txt");

    assert!(
        cx.debug_bounds("merge-base-toggle").is_some(),
        "a diff3 conflict offers the Base toggle"
    );
    assert!(
        cx.debug_bounds("merge-base").is_none(),
        "Base starts hidden"
    );
    click(cx, "merge-base-toggle");
    assert!(
        cx.debug_bounds("merge-base").is_some(),
        "the toggle shows the Base pane"
    );
    if cx.debug_bounds("merge-layout").is_some() {
        click(cx, "merge-layout");
        assert!(cx.debug_bounds("merge-local").is_some());
        assert!(cx.debug_bounds("merge-base").is_some());
        assert!(cx.debug_bounds("merge-incoming").is_some());
        assert!(cx.debug_bounds("merge-result-scroll").is_some());
    }
}

// ---------------------------------------------------------------------------
// 8. Dirty working tree: no merge state, no band, the store's message
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn a_dirty_tree_merge_refusal_draws_no_band_and_toasts(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = Scratch::new("dirty");
    repo.write("shared.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("shared.txt", "feat\n");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("shared.txt", "local work\n");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-dirty");
    let (_window, cx) = open_window!(cx, &store);

    open_feature_row_menu(cx);
    click(cx, "branch-menu-merge");
    assert_eq!(merge_sources(&drain(&net_rx)), vec!["feat".to_string()]);
    let outcome = settle_merge(cx, &store, &repo, "feat");
    let MergeOutcome::Failed(error) = outcome else {
        panic!("a dirty tree must refuse the merge, got {outcome:?}");
    };
    let error = error.unwrap_or_default();
    assert!(
        error.contains("would be overwritten"),
        "the net thread carries git's refusal: {error:?}"
    );
    assert!(!repo.has_merge_head());
    assert!(
        cx.debug_bounds("merge-band").is_none(),
        "a refusal has no band"
    );
    assert!(
        cx.read_entity(&store, |store, _| store.merge_state().is_none()),
        "no merge state"
    );
    assert_eq!(repo.read("shared.txt"), "local work\n");
    let texts = notices(&store, cx);
    assert!(
        texts
            .iter()
            .any(|text| text.contains("Merge blocked for merge-repo")),
        "the store's overwrite message: {texts:?}"
    );
    assert!(
        texts
            .iter()
            .any(|text| text.contains("uncommitted changes would be overwritten")),
        "the refusal is named, not echoed as a conflict: {texts:?}"
    );
}

// ---------------------------------------------------------------------------
// 9. Delete/modify: no text file, so the editor has nothing to show
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn a_delete_modify_conflict_opens_an_empty_editor(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = Scratch::new("delete-modify-ui");
    repo.write("gone.txt", "base\n");
    repo.write("keep.txt", "keep\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.git(&["rm", "-q", "gone.txt"]);
    repo.commit_all("feat deletes");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("gone.txt", "main edited\n");
    repo.commit_all("main edits");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-delete-modify");
    let (_window, cx) = open_window!(cx, &store);
    assert_eq!(merge_conflicted(&store, cx), ["gone.txt"]);
    assert!(cx.debug_bounds("u-gone.txt").is_some());

    click(cx, "open-merge-editor");
    assert_eq!(load_requests(&drain(&net_rx)), ["gone.txt"]);
    // `conflict_file` returns None: the net thread would send `file: None`.
    load_conflict(cx, &store, &repo, "gone.txt");
    assert!(
        cx.debug_bounds("merge-editor").is_some(),
        "the editor stays up on its loading surface"
    );
    assert!(
        cx.debug_bounds("merge-local").is_none(),
        "there is no Local pane for a missing stage"
    );
    assert!(
        cx.debug_bounds("merge-accept").is_none(),
        "Accept is not offered"
    );
    // A second draw must not panic on the empty state.
    draw(cx);
    draw(cx);
    click(cx, "merge-back");
    assert!(cx.debug_bounds("merge-editor").is_none());

    // The row menu's bulk take cannot resolve it either.
    right_click(cx, "u-gone.txt");
    click(cx, "file-menu-local");
    let commands = drain(&net_rx);
    assert_eq!(
        bulk_commands(&commands),
        vec![("gone.txt".to_string(), ConflictSide::Local)]
    );
    let bulk = settle_bulk(cx, &store, &repo, "gone.txt", ConflictSide::Local);
    assert!(!bulk.ok, "delete/modify has no text to take");
    assert!(
        bulk.error
            .as_deref()
            .is_some_and(|error| error.contains("no text conflict")),
        "{:?}",
        bulk.error
    );
    assert!(
        cx.debug_bounds("u-gone.txt").is_some(),
        "the row is still conflicted after the failed take"
    );
    assert!(
        notices(&store, cx)
            .iter()
            .any(|text| text.contains("no text conflict")),
        "{:?}",
        notices(&store, cx)
    );
    // The row's generic (hover) Stage verb is the only way to clear such an
    // entry, and it stages whatever the worktree holds.
    let row = cx
        .debug_bounds("u-gone.txt")
        .expect("the conflicted row draws");
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    draw(cx);
    assert!(
        cx.debug_bounds("toggle-false-gone.txt").is_some(),
        "hovering the row still offers its stage verb"
    );
    // That generic verb is the workaround: `git add` stages the worktree's
    // surviving side and clears the unmerged entry.
    click(cx, "toggle-false-gone.txt");
    let commands = drain(&net_rx);
    assert!(
        commands.iter().any(|command| matches!(
            command,
            NetCommand::Git {
                action: GitAction::Stage(path),
                ..
            } if path == "gone.txt"
        )),
        "the hover verb stages the conflicted path"
    );
    let staged = block_on(cli::stage(repo.path(), "gone.txt"));
    git_done(cx, &store, &repo, "stage", None, staged.error.clone());
    reload_changes(cx, &store, &repo);
    assert!(cx.debug_bounds("u-gone.txt").is_none());
    assert!(merge_conflicted(&store, cx).is_empty());
}

// ---------------------------------------------------------------------------
// 10. Binary conflicts: the same empty editor, no crash
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn a_binary_conflict_has_no_editor_and_no_way_out_through_the_ui(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = Scratch::new("binary-ui");
    repo.write_bytes("bin.dat", b"\x00base\x01");
    repo.write("text.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write_bytes("bin.dat", b"\x00feat\x02");
    repo.commit_all("feat binary");
    repo.git(&["checkout", "-q", "main"]);
    repo.write_bytes("bin.dat", b"\x00main\x03");
    repo.commit_all("main binary");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-binary");
    let (_window, cx) = open_window!(cx, &store);
    assert!(cx.debug_bounds("u-bin.dat").is_some());

    click(cx, "open-merge-editor");
    assert_eq!(load_requests(&drain(&net_rx)), ["bin.dat"]);
    load_conflict(cx, &store, &repo, "bin.dat");
    draw(cx);
    assert!(cx.debug_bounds("merge-editor").is_some());
    assert!(cx.debug_bounds("merge-local").is_none());
    assert!(cx.debug_bounds("merge-accept").is_none());

    click(cx, "merge-back");
    right_click(cx, "u-bin.dat");
    click(cx, "file-menu-incoming");
    let bulk = settle_bulk(cx, &store, &repo, "bin.dat", ConflictSide::Incoming);
    assert!(!bulk.ok);
    assert!(
        cx.debug_bounds("u-bin.dat").is_some(),
        "still conflicted: the UI cannot discard or take a binary side"
    );
}

// ---------------------------------------------------------------------------
// 11. Merge disabled while stopped; Abort restores the tree
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn abort_restores_the_tree_and_the_menu_stays_disabled_while_stopped(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = divergent_repo("abort");
    let head = repo.head();
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-abort");
    let (_window, cx) = open_window!(cx, &store);

    open_feature_row_menu(cx);
    click(cx, "branch-menu-merge");
    assert!(merge_sources(&drain(&net_rx)).is_empty());
    close_row_menu(cx);

    click(cx, "merge-abort");
    assert_eq!(abort_commands(&drain(&net_rx)), 1);
    let aborted = settle_abort(cx, &store, &repo);
    assert!(aborted.ok, "{:?}", aborted.error);
    assert!(!repo.has_merge_head());
    assert_eq!(repo.head(), head);
    assert_eq!(repo.read("shared.txt"), "main\n");
    assert_eq!(repo.status_short(), "");
    assert!(
        cx.debug_bounds("merge-band").is_none(),
        "the band leaves with the abort"
    );
    assert!(merge_conflicted(&store, cx).is_empty());
    assert!(
        notices(&store, cx)
            .iter()
            .any(|notice| notice == "Merge aborted"),
        "{:?}",
        notices(&store, cx)
    );
}

// ---------------------------------------------------------------------------
// 12. Keyboard: F7 between blocks, F10/F11 take-alls, Ctrl+Enter accept, Esc
// ---------------------------------------------------------------------------

#[gpui_kit::gpui::test]
fn the_editor_keys_step_and_take_the_sides(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = two_block_repo("keys");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-keys");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "multi.txt");
    assert!(cx.debug_bounds("merge-block-0").is_some());
    assert!(cx.debug_bounds("merge-block-1").is_some());

    // Block 0: incoming. F7 steps to block 1; there: local. Accept then has
    // one side from each block.
    click(cx, "merge-incoming-take");
    cx.simulate_keystrokes("f7");
    draw(cx);
    click(cx, "merge-local-take");
    click(cx, "merge-accept");
    let commands = drain(&net_rx);
    assert_eq!(
        resolve_commands(&commands),
        vec![(
            "multi.txt".to_string(),
            Some("one\nfeat1\ntwo\nthree\nfour\nfive\nmain2\nend\n".to_string())
        )],
        "F7 moved the block cursor to block 1"
    );
}

#[gpui_kit::gpui::test]
fn f10_and_ctrl_enter_take_incoming_for_the_whole_file(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = two_block_repo("take-alls");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-take-alls");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "multi.txt");
    // The first press inside the editor puts keyboard focus on its surface.
    click(cx, "merge-local");
    cx.simulate_keystrokes("f10");
    draw(cx);
    cx.simulate_keystrokes("ctrl-enter");
    draw(cx);
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![(
            "multi.txt".to_string(),
            Some("one\nfeat1\ntwo\nthree\nfour\nfive\nfeat2\nend\n".to_string())
        )],
        "F10 took Incoming for both blocks and Ctrl+Enter accepted"
    );
}

#[gpui_kit::gpui::test]
fn f11_takes_local_and_escape_closes_without_accepting(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = two_block_repo("take-local");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-take-local");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "multi.txt");
    click(cx, "merge-local");
    cx.simulate_keystrokes("f11");
    draw(cx);
    click(cx, "merge-accept");
    assert_eq!(
        resolve_commands(&drain(&net_rx)),
        vec![(
            "multi.txt".to_string(),
            Some("one\nmain1\ntwo\nthree\nfour\nfive\nmain2\nend\n".to_string())
        )],
        "F11 took Local for both blocks"
    );
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "multi.txt");
    click(cx, "merge-local");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("merge-editor").is_none(),
        "Escape closes the editor"
    );
}

/// The same fixture that used to crash the Local pane: stage 2 line `B` was
/// handed the highlight span of `CONFLICT_LONG`. The per-stage mapping and the
/// run filter keep the editor drawable.
#[gpui_kit::gpui::test]
fn a_hoisted_prefix_conflict_opens_without_panicking(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = hoisted_prefix_repo("min-crash");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-min-crash");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    assert_eq!(load_requests(&drain(&net_rx)), ["f.txt"]);
    load_conflict(cx, &store, &repo, "f.txt");
    assert!(
        cx.debug_bounds("merge-local").is_some(),
        "the Local pane draws for a hoisted prefix"
    );
}

// ---------------------------------------------------------------------------
// 13. The owner's stopped `.gitignore` merge, copied read-only into /tmp
// ---------------------------------------------------------------------------

/// The fixture is home-relative: a checkout carrying a stopped merge with the
/// hoisted-prefix shape. A machine without it skips the probe.
fn owner_fixture() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let source = std::path::PathBuf::from(home).join("Developments/KwmSwitcher");
    source.join(".git/MERGE_HEAD").is_file().then_some(source)
}

/// `cp -a` minus `target/`: `/tmp` cannot hold the owner's 23G build tree, and
/// the merge state lives in `.git` plus the small worktree. The source is only
/// read.
fn copy_owner_fixture(name: &str) -> Option<Scratch> {
    let Some(source) = owner_fixture() else {
        eprintln!("skipping {name}: no stopped merge fixture under $HOME/Developments");
        return None;
    };
    let source = source.as_path();
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let dest = std::env::temp_dir().join(format!(
        "merge-deep-ui-kwm-{name}-{}-{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).expect("copy dir");
    let payload = Command::new("tar")
        .arg("-C")
        .arg(source)
        .arg("--exclude=./target")
        .arg("-cf")
        .arg("-")
        .arg(".")
        .output()
        .expect("tar reads the fixture");
    assert!(payload.status.success(), "tar copy failed");
    let mut unpack = Command::new("tar")
        .arg("-C")
        .arg(&dest)
        .arg("-xf")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("tar unpacks");
    {
        use std::io::Write;
        unpack
            .stdin
            .as_mut()
            .expect("tar stdin")
            .write_all(&payload.stdout)
            .expect("tar payload");
    }
    let unpack = unpack.wait_with_output().expect("untar finishes");
    assert!(unpack.status.success(), "untar failed");
    assert!(dest.join(".git/MERGE_HEAD").is_file());
    Some(Scratch::existing(dest))
}

#[gpui_kit::gpui::test]
fn the_owners_stopped_merge_draws_the_band_rows_and_branch_menu(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let Some(repo) = copy_owner_fixture("band") else {
        return;
    };
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-owner-band");
    let (_window, cx) = open_window!(cx, &store);
    draw(cx);
    draw(cx);

    assert_eq!(merge_conflicted(&store, cx), [".gitignore"]);
    assert!(
        cx.debug_bounds("merge-band").is_some(),
        "the owner's band renders over the big status list"
    );
    // The list is virtualised and 39 staged adds/deletes sit above the
    // conflict. Focus the list from a visible staged row, End to the bottom,
    // then select the conflicted row.
    assert!(
        cx.debug_bounds("u-.vscode/extensions.json").is_none(),
        "a clean staged add is a staged row, not an unstaged one"
    );
    assert!(cx.debug_bounds("s-.vscode/extensions.json").is_some());
    click(cx, "s-.vscode/extensions.json");
    cx.simulate_keystrokes("end");
    draw(cx);
    assert!(
        cx.debug_bounds("u-.gitignore").is_some(),
        "the conflicted row is reachable after scrolling the staged list"
    );
    click(cx, "u-.gitignore");
    drain(&net_rx);
    assert!(
        cx.debug_bounds("open-merge-editor").is_some(),
        "selecting the row opens the conflict pane"
    );

    // The branch dropdown over 80 rows and the band: draw it and close it.
    click(cx, "branch-button");
    draw(cx);
    assert!(cx.debug_bounds("branch-menu").is_some());
    close_row_menu(cx);

    // Opening the editor before its file lands draws the loading surface.
    click(cx, "open-merge-editor");
    assert_eq!(load_requests(&drain(&net_rx)), [".gitignore"]);
    assert!(cx.debug_bounds("merge-editor").is_some());
    assert!(
        cx.debug_bounds("merge-local").is_none(),
        "the panes wait for the read"
    );
    draw(cx);
    click(cx, "merge-back");
    assert!(cx.debug_bounds("merge-editor").is_none());
}

/// The aligned diff's rows: base `a,b,c`, local `a,b,LONG`, incoming
/// `b,c,Z`. The sides must sit on the base's rows, and a side that lacks the
/// line must be a gap on that row, not shifted text.
#[gpui_kit::gpui::test]
fn the_diff_aligns_the_sides_on_the_base(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let repo = hoisted_prefix_repo("aligned-diff");
    stop_with_merge(&repo, "feat");
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-aligned-diff");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    drain(&net_rx);
    load_conflict(cx, &store, &repo, "f.txt");

    // row 0: the base's `a` on the local side, the incoming side's gap
    assert!(
        cx.debug_bounds("merge-diff-local-0-1").is_some(),
        "the local line a sits on the base's first row"
    );
    assert!(
        cx.debug_bounds("merge-diff-incoming-0-gap").is_some(),
        "the incoming side has no a, so its cell is a gap"
    );
    // row 2: the replacement pairs CONFLICT_LONG with C
    assert!(cx.debug_bounds("merge-diff-local-2-3").is_some());
    assert!(cx.debug_bounds("merge-diff-incoming-2-2").is_some());
    // row 3: the incoming side's trailing Z has its own row, opposite a gap
    assert!(cx.debug_bounds("merge-diff-local-3-gap").is_some());
    assert!(cx.debug_bounds("merge-diff-incoming-3-3").is_some());
}

/// Loading the owner's `.gitignore` conflict used to panic while prepainting
/// the whole-file Local pane. The worktree's block is preceded by 8 lines that
/// belong to the incoming stage only, so the parser's `local_lines` (8) was not
/// where stage 2 keeps the block (57); the word highlight for
/// `///////////conflict` (0..19) was then applied to stage 2's line 8,
/// `## Build results` (16 bytes), and gpui refused the run. The per-stage
/// mapping locates the section exactly, and the run filter is the guard.
#[gpui_kit::gpui::test]
fn the_owners_gitignore_conflict_opens_the_editor_without_panicking(cx: &mut TestAppContext) {
    scratch_home();
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let Some(repo) = copy_owner_fixture("crash") else {
        return;
    };
    let (store, net_rx) = probe_store(cx, &repo, "merge-deep-owner-crash");
    let (_window, cx) = open_window!(cx, &store);
    click(cx, "open-merge-editor");
    assert_eq!(load_requests(&drain(&net_rx)), [".gitignore"]);
    // Prepainting the editor with the file's blocks used to panic here.
    load_conflict(cx, &store, &repo, ".gitignore");
    assert!(
        cx.debug_bounds("merge-local").is_some(),
        "the whole-file Local pane draws"
    );
    assert!(
        cx.debug_bounds("merge-incoming").is_some(),
        "the whole-file Incoming pane draws"
    );
    assert!(
        cx.debug_bounds("merge-diff-file").is_some(),
        "the base-aligned diff list draws"
    );
    let result = cx
        .debug_bounds("merge-result")
        .expect("the Result editor draws");
    assert!(
        f32::from(result.size.height) > 200.,
        "the Result area fills its pane (got {})",
        f32::from(result.size.height)
    );
}
