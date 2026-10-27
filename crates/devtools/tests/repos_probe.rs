//! Headless probe for the repos table: a real Store + MainWindow pair drawn
//! in a test window, driven through the kit's own selection path — the row
//! click handler, the TableEvent subscription and the store's selection all
//! verified without a display.

use devtools::store::Store;
use devtools::ui::window::MainWindow;
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::TestAppContext;
use std::path::PathBuf;

fn entry(name: &str, path: &str, tags: &[&str]) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: tags.iter().map(|t| t.to_string()).collect(),
        git_branch_name: Some("main".into()),
        git_modified_count: 3,
        git_to_push_count: 1,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: true,
        git_branch_published: true,
        git_status_loaded: true,
        github_pr_count: 0,
        github_issue_count: 0,
    }
}

fn fake_store(cx: &mut TestAppContext) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &["favorites"]),
            entry("beta", "/probe/beta", &[]),
            entry("gamma", "/probe/gamma", &[]),
        ]);
        store
    })
}

#[gpui::test]
fn row_selection_reaches_the_store(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // the centering wrapper fills the row minus the kit's own 4+4 cell
    // padding (48px row → 40px column), rows advance by one row height, and
    // the name cell sits centered inside its row
    let wrap0 = cx.debug_bounds("repos-td-wrap-0").expect("row 0 renders");
    let wrap1 = cx.debug_bounds("repos-td-wrap-1").expect("row 1 renders");
    assert!(
        (wrap0.size.height - gpui::px(40.)).abs() < gpui::px(2.),
        "cells fill the row minus the kit's cell padding (got {:?})",
        wrap0.size.height
    );
    let row_pitch = wrap1.origin.y - wrap0.origin.y;
    assert!(
        (row_pitch - gpui::px(48.)).abs() < gpui::px(2.),
        "rows advance by the 48px row height (got {row_pitch})",
    );
    let cell = cx
        .debug_bounds("repos-name-cell-0")
        .expect("name cell renders");
    let cell_mid = cell.origin.y + cell.size.height / 2.;
    let wrap_mid = wrap0.origin.y + wrap0.size.height / 2.;
    assert!(
        (cell_mid - wrap_mid).abs() < gpui::px(2.),
        "the name cell centers in its row (cell mid {cell_mid} vs row mid {wrap_mid})"
    );

    // the table's own selected-row API drives the same path a click does
    let selected_before = cx.read_entity(&store, |s, _| s.selected_repo.clone());
    assert!(selected_before.is_none());

    // click into the first row through the real event pipeline
    cx.simulate_mouse_down(gpui::point(px_col(), px_row()), gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(gpui::point(px_col(), px_row()), gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let selected = cx.read_entity(&store, |s, _| s.selected_repo.clone());
    assert_eq!(
        selected.as_deref(),
        Some("/probe/alpha"),
        "clicking the first row should select alpha (got {selected:?})"
    );
}

/// A dialog is a modal over the whole workspace: while it is open the page
/// takes the body height (the bar would otherwise paint over the dialog's
/// footer) and Escape brings the bar back.
#[gpui::test]
fn add_dialog_covers_the_workspace_and_escape_restores_it(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/alpha".into()), cx)
    });
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.debug_bounds("bar-root").is_some(), "bar starts open");

    let add = cx
        .debug_bounds("add-repo-button")
        .expect("add-repository button in the toolbar");
    cx.simulate_click(
        gpui::point(add.origin.x + add.size.width / 2., add.origin.y + add.size.height / 2.),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "dialog is open",
    );
    assert!(
        cx.debug_bounds("bar-root").is_none(),
        "the bar must yield the body height to the dialog",
    );

    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("add-repo-overlay").is_none(),
        "Escape closes the dialog",
    );
    assert!(
        cx.debug_bounds("bar-root").is_some(),
        "the bar comes back after the dialog closes",
    );
}

#[gpui::test]
fn selection_survives_and_closes_via_the_store(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/beta".into()), cx)
    });
    cx.run_until_parked();
    assert_eq!(cx.read_entity(&store, |s, _| s.selected_repo.clone()).as_deref(), Some("/probe/beta"));
    assert!(cx.read_entity(&store, |s, _| s.selected_entry().is_some()));

    cx.update_entity(&store, |s: &mut Store, cx| s.select_repo(None, cx));
    assert!(cx.read_entity(&store, |s, _| s.selected_repo.is_none()));
}

/// App alerts surface as toasts in the window's bottom-right corner, not as
/// inline header text: a store push renders a severity card, and its close
/// button removes exactly that card.
#[gpui::test]
fn toasts_render_stack_and_dismiss(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.notify_error("Pull failed for warp-agent: cannot pull with rebase");
        s.notify_success("Fetched alpha");
        cx.notify();
    });

    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.debug_bounds("toast-stack").is_some(), "the toast stack renders");
    assert!(cx.debug_bounds("toast-0").is_some(), "the error toast renders");
    assert!(cx.debug_bounds("toast-1").is_some(), "the success toast renders");

    let close = cx
        .debug_bounds("toast-close-1")
        .expect("each toast carries a close button");
    cx.simulate_click(close.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.debug_bounds("toast-1").is_none(), "the dismissed toast is gone");
    assert!(cx.debug_bounds("toast-0").is_some(), "the sibling toast stays");
}

/// The branch dropdown follows the kebab menu's dismissal contract: Escape
/// closes it, a click anywhere outside the card closes it, and picking a
/// branch closes it — only the toggle button reopens it.
#[gpui::test]
fn branch_menu_closes_on_escape_outside_and_select(cx: &mut TestAppContext) {
    use devtools_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/alpha".into()), cx);
        // the branch selector lives on the Changes tab
        s.set_tab(devtools::store::BottomBarTab::Changes, cx);
        s.branches = vec![
            GitBranchRef { name: "main".into(), kind: GitBranchKind::Local },
            GitBranchRef { name: "feature".into(), kind: GitBranchKind::Local },
        ];
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let open_menu = |cx: &mut gpui::VisualTestContext| {
        let button = cx
            .debug_bounds("branch-button")
            .expect("the branch selector renders in the rail");
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| drop(window.draw(cx)));
    };

    open_menu(cx);
    assert!(cx.debug_bounds("branch-menu").is_some(), "the dropdown opens");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "Escape closes the dropdown"
    );

    // a click on the tab strip is outside the card: it dismisses the menu
    open_menu(cx);
    let bar = cx.debug_bounds("bar-root").expect("the bar renders");
    cx.simulate_click(
        gpui::point(bar.origin.x + gpui::px(60.), bar.origin.y + gpui::px(30.)),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "clicking outside the card closes the dropdown"
    );

    // picking a branch checks it out and closes the dropdown
    open_menu(cx);
    let row = cx
        .debug_bounds("branch-row-feature")
        .expect("the filtered branch rows render");
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "picking a branch closes the dropdown"
    );
    assert!(
        cx.read_entity(&store, |s, _| s.git_busy),
        "the checkout went out"
    );

    // the toggle button reopens after all of the above
    open_menu(cx);
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the dropdown reopens from the button"
    );
}

/// The dropdown groups local branches first, then an inert "Remote" label,
/// then the remote-tracking branches.
#[gpui::test]
fn branch_menu_groups_locals_above_remotes(cx: &mut TestAppContext) {
    use devtools_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/alpha".into()), cx);
        s.set_tab(devtools::store::BottomBarTab::Changes, cx);
        s.branches = vec![
            GitBranchRef { name: "main".into(), kind: GitBranchKind::Local },
            GitBranchRef { name: "feature".into(), kind: GitBranchKind::Local },
            GitBranchRef { name: "origin/main".into(), kind: GitBranchKind::Remote },
            GitBranchRef { name: "origin/rust".into(), kind: GitBranchKind::Remote },
        ];
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let button = cx
        .debug_bounds("branch-button")
        .expect("the branch selector renders in the rail");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.debug_bounds("branch-menu").is_some(), "the dropdown opens");
    let label = cx
        .debug_bounds("branch-group-remote")
        .expect("the Remote group label renders");
    let local = cx.debug_bounds("branch-row-main").expect("the local row renders");
    let remote = cx
        .debug_bounds("branch-row-origin/main")
        .expect("the remote row renders");
    assert!(
        local.bottom() <= label.origin.y,
        "locals render above the Remote label"
    );
    assert!(
        label.bottom() <= remote.origin.y,
        "the Remote label renders above the remotes"
    );
}

/// The warp status bridge: pushes land per folder, and availability-loss
/// clears them — the row hints follow warp-agent down with it.
#[gpui::test]
fn warp_status_tracks_running_workspaces(cx: &mut TestAppContext) {
    use devtools::net::{UiEvent, WarpWorkspaceStatus};

    cx.update(gpui_kit::init);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::WarpStatus {
                workspaces: vec![(
                    "/repos/a".into(),
                    WarpWorkspaceStatus { connected: true, running: vec!["chat one".into(), "chat two".into()] },
                )],
            },
            cx,
        );
        s.on_net_event(UiEvent::WarpAvailability { connected: true }, cx);
    });

    assert!(cx.read_entity(&store, |s, _| s.warp_connected));
    assert_eq!(
        cx.read_entity(&store, |s, _| s
            .warp_running
            .get("/repos/a")
            .map(|w| w.running.clone()))
        .unwrap(),
        vec!["chat one".to_string(), "chat two".to_string()]
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(UiEvent::WarpAvailability { connected: false }, cx);
    });
    assert!(!cx.read_entity(&store, |s, _| s.warp_connected));
    assert!(cx.read_entity(&store, |s, _| s.warp_running.is_empty()));
}

/// The Changes-tab handoff leaves as a WarpAsk command carrying the repo's
/// folder. The store is backed by a scratch settings file pointing the
/// integration at an existing executable, so the gate passes.
#[gpui::test]
fn ask_agent_hands_off_through_the_door(cx: &mut TestAppContext) {
    use devtools::net::NetCommand;

    cx.update(gpui_kit::init);

    let exe = std::env::current_exe().expect("test binary path");
    let json = format!(
        r#"{{"OpenCode": {{"EnableOpenCode": true}}, "WarpAgent": {{"EnableWarpAgent": true, "Executable": "{}"}}}}"#,
        exe.display().to_string().replace('\\', "\\\\")
    );
    let scratch = std::env::temp_dir().join(format!("devtools-probe-warp-{}.json", std::process::id()));
    std::fs::write(&scratch, json).expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        Store::with_settings(
            net_tx,
            std::sync::Arc::new(devtools_core::settings::SettingsService::at(scratch.clone())),
        )
    });
    cx.update_entity(&store, |s: &mut Store, _| {
        s.selected_repo = Some("/repos/a".into());
    });
    cx.update_entity(&store, |s, cx| s.ask_warp_about_changes(cx));
    cx.run_until_parked();

    let mut handed_off = false;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::WarpAsk { folder, .. } = command {
            assert_eq!(folder, std::path::PathBuf::from("/repos/a"));
            handed_off = true;
        }
    }
    assert!(handed_off, "the handoff command went out");

    let _ = std::fs::remove_file(&scratch);
}

/// Switching repos resets the whole per-repo workspace: snapshot, branches,
/// history, drafts, wand state and the busy flags — the bar never shows the
/// previous repo's detail.
#[gpui::test]
fn switching_repos_resets_the_workspace(cx: &mut TestAppContext) {
    use devtools::store::BottomBarTab;
    use devtools_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/alpha".into()), cx);
        // prime everything the switch must clear (repo A's state)
        s.set_changes(None);
        s.branches = vec![GitBranchRef { name: "main".into(), kind: GitBranchKind::Local }];
        s.recent_commits = vec![devtools_core::model::GitCommitInfo {
            hash: "abc".into(),
            subject: "old".into(),
            author: None,
            date: chrono::Local::now(),
        }];
        s.unpushed = Some(["abc".to_string()].into_iter().collect());
        s.last_git_error = Some("stale".into());
        s.commit_message_draft = Some("A's draft".into());
        s.readme_draft = Some("A's readme".into());
        s.wand_busy = Some(devtools::net::WandKind::Commit);
        s.wand_repo = Some("/probe/alpha".into());
        s.wand_error = Some("stale".into());
        s.git_busy = true;
        s.selected_tab = BottomBarTab::PullRequests;
    });

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/beta".into()), cx);
    });
    cx.run_until_parked();

    cx.read_entity(&store, |s, _| {
        assert_eq!(s.selected_repo.as_deref(), Some("/probe/beta"));
        assert_eq!(s.selected_tab, BottomBarTab::Overview);
        assert!(s.branches.is_empty(), "branches reset");
        assert!(s.recent_commits.is_empty(), "history reset");
        assert!(s.unpushed.is_none(), "unpushed reset");
        assert!(s.last_git_error.is_none(), "git error reset");
        assert!(s.commit_message_draft.is_none(), "commit draft reset");
        assert!(s.readme_draft.is_none(), "readme draft reset");
        assert!(s.wand_error.is_none(), "wand error reset");
        assert!(!s.git_busy, "git busy reset");
        // the in-flight run keeps its busy flag but loses its repo: it can
        // no longer land a draft into beta's bar
        assert_eq!(s.wand_repo, None);
    });
}

/// A wand run kicked for repo A never lands its draft once repo B is
/// selected — the run ends, but beta's commit box stays empty.
#[gpui::test]
fn wand_done_for_a_switched_away_repo_lands_nothing(cx: &mut TestAppContext) {
    use devtools::net::{UiEvent, WandKind};

    cx.update(gpui_kit::init);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/alpha".into()), cx);
    });
    // the run was kicked for alpha, the user has since moved to beta
    cx.update_entity(&store, |s, _| {
        s.wand_busy = Some(WandKind::Commit);
        s.wand_repo = Some("/probe/alpha".into());
    });
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.select_repo(Some("/probe/beta".into()), cx);
        s.wand_busy = Some(WandKind::Commit);
        s.wand_repo = Some("/probe/alpha".into());
    });
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.on_net_event(
            UiEvent::WandDone {
                wand_id: "w".into(),
                kind: WandKind::Commit,
                result: Ok("A's generated message".into()),
            },
            cx,
        );
    });

    cx.read_entity(&store, |s, _| {
        assert!(s.wand_busy.is_none(), "the run ended");
        assert!(
            s.commit_message_draft.is_none(),
            "alpha's draft never lands in beta's bar"
        );
    });

    // for comparison: a run for the still-selected repo lands its draft
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.wand_busy = Some(WandKind::Commit);
        s.wand_repo = Some("/probe/beta".into());
        s.on_net_event(
            UiEvent::WandDone {
                wand_id: "w".into(),
                kind: WandKind::Commit,
                result: Ok("B's generated message".into()),
            },
            cx,
        );
    });
    cx.read_entity(&store, |s, _| {
        assert_eq!(
            s.commit_message_draft.as_deref(),
            Some("B's generated message")
        );
    });
}

/// Table rows center their content: the first row's name cell sits on the
/// row's vertical center. The td wrapper must leave the kit's own 4+4 cell
/// padding free — at the full row height it overflows 8px and everything
/// rides ~4px low.
#[gpui::test]
fn table_rows_center_their_content(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let header = cx.debug_bounds("repos-header").expect("table header renders");
    let name = cx
        .debug_bounds("repos-name-cell-0")
        .expect("the first row renders");
    let row_center = header.bottom() + gpui::px(devtools::theme::TABLE_ROW_H / 2.);
    let cell_center = name.origin.y + name.size.height / 2.;
    assert!(
        (cell_center - row_center).abs() <= gpui::px(2.),
        "row content must sit on the row center (cell {cell_center} vs row {row_center})"
    );
}

/// The periodic fetch scheduler: a due schedule sends exactly one FetchAll
/// carrying only the repos with a remote, the selected repo's folder drops
/// out while a git mutation runs there, and each fire re-arms the next due
/// instant. DEVTOOLS_AUTO_FETCH_SECS=1 replaces the minutes setting so the
/// probe runs on the E2E interval.
#[gpui::test]
fn auto_fetch_fires_for_remote_repos_and_reschedules(cx: &mut TestAppContext) {
    use devtools::net::NetCommand;

    cx.update(gpui_kit::init);

    // SAFETY: process-global env; read wherever the schedule is computed, no
    // sibling test in this binary arms a tick to consume it
    unsafe { std::env::set_var("DEVTOOLS_AUTO_FETCH_SECS", "1") };

    let scratch = std::env::temp_dir().join(format!("devtools-auto-fetch-store-{}.json", std::process::id()));
    std::fs::write(&scratch, r#"{"Repos": {"AutoFetchMinutes": 30}}"#).expect("scratch settings");

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(devtools_core::settings::SettingsService::at(scratch.clone())),
        );
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha", &[]),
            entry("beta", "/probe/beta", &[]),
            entry("gamma", "/probe/gamma", &[]),
        ]);
        store.repos[1].git_has_remote = false;
        store
    });
    assert!(
        cx.read_entity(&store, |s, _| s.next_auto_fetch.is_some()),
        "construction arms the schedule"
    );

    // force the schedule due, then fire
    cx.update_entity(&store, |s: &mut Store, _| {
        s.next_auto_fetch = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
    });
    cx.update_entity(&store, |s: &mut Store, cx| s.check_auto_fetch(cx));

    let mut fetch_all: Option<Vec<PathBuf>> = None;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::FetchAll { folders } = command {
            assert!(fetch_all.is_none(), "exactly one FetchAll went out");
            fetch_all = Some(folders);
        }
    }
    assert_eq!(
        fetch_all.expect("the FetchAll command went out"),
        vec![PathBuf::from("/probe/alpha"), PathBuf::from("/probe/gamma")],
        "only the repos with a remote are fetched"
    );
    cx.read_entity(&store, |s, _| {
        let due = s.next_auto_fetch.expect("each fire re-arms the schedule");
        assert!(due > std::time::Instant::now(), "the next due sits in the future");
    });

    // a git mutation in the selected repo excludes its folder (index.lock)
    cx.update_entity(&store, |s: &mut Store, _| {
        s.selected_repo = Some("/probe/alpha".into());
        s.git_busy = true;
        s.next_auto_fetch = Some(std::time::Instant::now() - std::time::Duration::from_secs(1));
    });
    cx.update_entity(&store, |s: &mut Store, cx| s.check_auto_fetch(cx));
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::FetchAll { folders } = command {
            assert_eq!(folders, vec![PathBuf::from("/probe/gamma")], "the busy selected repo is excluded");
        }
    }

    unsafe { std::env::remove_var("DEVTOOLS_AUTO_FETCH_SECS") };
    let _ = std::fs::remove_file(&scratch);
}

fn px_col() -> gpui::Pixels {
    gpui::px(120.)
}

fn px_row() -> gpui::Pixels {
    // titlebar 38 + two 46px toolbar rows + 28px column header + row center
    // (48px rows)
    gpui::px(182.)
}

fn entry_with_branch(name: &str, path: &str, branch: &str) -> RepoEntry {
    let mut repo = entry(name, path, &[]);
    repo.git_branch_name = Some(branch.into());
    repo
}

/// The columns are sized dynamically: the branch column hugs the widest
/// branch in the projection and the name column absorbs whatever is left, so
/// the widths always fill the table's available width exactly.
#[gpui::test]
fn columns_distribute_the_available_width(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, _| {
        s.repos.push(entry_with_branch("delta", "/probe/delta", "release/feature-x"));
        // reproject also refreshes the widest-branch width the delegate reads
        s.reproject_pub();
    });

    cx.update(|cx| {
        let available = gpui::px(1100.);
        let delegate = devtools::ui::repos::ReposDelegate::new(store.clone(), available);
        use gpui_kit::component::table::TableDelegate as _;

        let count = delegate.columns_count(cx);
        let widths: Vec<gpui::Pixels> = (0..count).map(|ix| delegate.column(ix, cx).width).collect();
        let name = widths[1];
        let branch = widths[3]; // shortcuts sits between name and branch
        let others: gpui::Pixels = widths
            .iter()
            .enumerate()
            .filter(|(ix, _)| *ix != 1)
            .map(|(_, w)| *w)
            .sum();

        // branch hugs the widest branch: "release/feature-x" = 17 chars
        assert_eq!(branch, gpui::px(56. + 17. * 7.8));
        assert!(name >= gpui::px(140.), "the name column keeps its floor (got {name})");
        assert!(
            (others + name - available).abs() < gpui::px(1.),
            "columns must fill the available width exactly (others {others} + name {name} vs {available})"
        );

        // a huge branch clamps at the ceiling instead of starving the name
        store.update(cx, |s, _| {
            if let Some(repo) = s.repos.last_mut() {
                repo.git_branch_name =
                    Some("release/an-extremely-long-branch-name-for-clamping".into());
            }
            s.reproject_pub();
        });
        let branch = delegate.column(3, cx).width;
        assert_eq!(branch, gpui::px(240.), "branch width clamps at 240");
    });
}

/// Narrow windows drop the time/GitHub/Azure columns and finally shrink the
/// changes cell to its badge-only form; at every width the columns still fit
/// the table (no horizontal scrollbar) and the name keeps its floor.
#[gpui::test]
fn columns_fit_narrow_widths(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);

    let store = fake_store(cx);
    cx.update(|cx| {
        use gpui_kit::component::table::TableDelegate as _;

        for available in [674., 700., 760., 854., 960., 1100., 1234.] {
            let delegate = devtools::ui::repos::ReposDelegate::new(store.clone(), gpui::px(available));
            let count = delegate.columns_count(cx);
            let widths: Vec<gpui::Pixels> =
                (0..count).map(|ix| delegate.column(ix, cx).width).collect();
            let sum: gpui::Pixels = widths.iter().copied().sum();

            assert!(
                sum <= gpui::px(available) + gpui::px(1.),
                "columns must fit {available}px, got {widths:?} (sum {sum})"
            );
            assert!(
                widths[1] >= gpui::px(140.),
                "the name column keeps its floor at {available}px (got {})",
                widths[1]
            );
            assert!(
                widths[4] >= gpui::px(80.) && widths[4] <= gpui::px(154.),
                "the changes column stays usable at {available}px (got {})",
                widths[4]
            );
            assert_eq!(widths[0], gpui::px(36.), "star column leads at {available}px");
            assert!(
                widths[2] > gpui::px(100.),
                "launcher buttons stay at {available}px (got {})",
                widths[2]
            );
        }
    });
}

/// The toolbar's sort dropdown is gone: the table headers are the only sort
/// controls. A Branch header sort must reach the store as RepoSortMode::Branch
/// and reorder the projection. The header path persists the mode, so the
/// probe runs under a temp HOME.
#[gpui::test]
fn branch_header_sort_reorders_the_projection(cx: &mut TestAppContext) {
    use devtools_core::settings::RepoSortMode;
    use gpui_kit::component::table::{ColumnSort, TableDelegate as _};

    let home = std::env::temp_dir().join(format!("devtools-sort-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".devtools")).unwrap();
    // SAFETY: process-global env; other tests in this binary tolerate a
    // default-settings store, so a racing HOME read is harmless
    unsafe { std::env::set_var("HOME", &home) };

    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, _| {
        // alpha ships as a favorite; favorites float above every sort
        s.repos[0].tags.clear();
        s.repos[0].git_branch_name = Some("zeta".into());
        s.repos[1].git_branch_name = Some("alpha".into());
        // gamma keeps the entry default ("main")
    });

    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    cx.update(|window, cx| {
        let delegate = devtools::ui::repos::ReposDelegate::new(store.clone(), gpui::px(1100.));
        let branch_ix = (0..delegate.columns_count(cx))
            .find(|&ix| delegate.column(ix, cx).key == "branch")
            .expect("the branch column is visible at 1100px");
        let table = cx.new(|cx| {
            gpui_kit::component::table::TableState::new(delegate, window, cx)
        });
        table.update(cx, |state, cx| {
            state
                .delegate_mut()
                .perform_sort(branch_ix, ColumnSort::Ascending, window, cx);
        });
    });

    let (mode, ascending) = cx.read_entity(&store, |s, _| (s.sort_mode, s.sort_ascending));
    assert_eq!(mode, RepoSortMode::Branch, "the branch header maps to the Branch mode");
    assert!(ascending, "Ascending lands as ascending");
    let order = cx.read_entity(&store, |s, _| {
        s.projected.iter().map(|&i| s.repos[i].name.clone()).collect::<Vec<_>>()
    });
    assert_eq!(
        order,
        vec!["beta", "gamma", "alpha"],
        "branches alpha < main < zeta order the rows"
    );

    let _ = std::fs::remove_dir_all(&home);
}

fn scanned(name: &str, path: &str) -> devtools_core::scan::ScannedRepo {
    devtools_core::scan::ScannedRepo {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
    }
}

fn dialog_click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing from the frame"));
    cx.simulate_click(b.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn dialog_draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| drop(window.draw(cx)));
}

/// The Add dialog: results arrive through the store (the net settle path),
/// Add stays gated until a row is checked — a no-op click must not close the
/// dialog — and Rescan resets to the Scan state through the real field +
/// button interaction.
#[gpui::test]
fn add_dialog_gated_add_and_rescan(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    dialog_draw(cx);

    dialog_click(cx, "add-repo-button");
    assert!(cx.debug_bounds("add-repo-overlay").is_some(), "dialog opens");
    assert!(cx.debug_bounds("btn-Scan").is_some(), "the dedicated Scan button renders");
    assert!(
        cx.debug_bounds("add-dialog-close").is_some(),
        "the header close button renders"
    );

    // the net settle path: the worker drops results on the store, the page
    // drains them while drawing
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.deep_scan_result = Some(vec![
            scanned("repo-x", "/probe/scan/repo-x"),
            scanned("repo-y", "/probe/scan/repo-y"),
        ]);
        cx.notify();
    });
    cx.run_until_parked();
    dialog_draw(cx);
    assert!(cx.debug_bounds("btn-accent-Add").is_some(), "Add waits in the footer");
    assert!(cx.debug_bounds("found-repo-0").is_some());

    // uncheck both rows: the disabled Add must be a no-op, not a close
    dialog_click(cx, "found-repo-0");
    dialog_click(cx, "found-repo-1");
    dialog_click(cx, "btn-accent-Add");
    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "gated Add must not close the dialog"
    );

    // type a fresh path and Scan again: stale rows are cleared, the dialog
    // holds; the results well is back to its empty text
    dialog_click(cx, "add-root-input");
    cx.simulate_input("/probe/scan-root");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-Scan");
    assert!(
        cx.debug_bounds("found-repo-0").is_none(),
        "stale results are cleared on rescan"
    );
    assert!(cx.debug_bounds("add-repo-overlay").is_some());

    // the header X closes the dialog
    dialog_click(cx, "add-dialog-close");
    assert!(
        cx.debug_bounds("add-repo-overlay").is_none(),
        "the X close button dismisses the dialog"
    );
}

/// The Clone dialog: the destination is editable and feeds validation — an
/// empty destination holds the dialog open with the inline error, a valid
/// one lets the commit flip the button to its busy label.
#[gpui::test]
fn clone_dialog_editable_destination_feeds_validation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    dialog_draw(cx);

    dialog_click(cx, "clone-button");
    assert!(cx.debug_bounds("clone-overlay").is_some(), "dialog opens");
    assert!(cx.debug_bounds("btn-accent-Clone").is_some());

    // no URL yet: the disabled Clone is a no-op
    dialog_click(cx, "btn-accent-Clone");
    assert!(cx.debug_bounds("clone-overlay").is_some());

    // URL typed, destination cleared: validation must hold the dialog open
    dialog_click(cx, "clone-url-input");
    cx.simulate_input("https://github.com/owner/dt-probe-clone.git");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "clone-dest-input");
    cx.simulate_keystrokes("ctrl-a backspace");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-accent-Clone");
    assert!(
        cx.debug_bounds("clone-overlay").is_some(),
        "a missing destination keeps the dialog open"
    );
    assert!(
        cx.debug_bounds("btn-accent-Cloning…").is_none(),
        "the clone must not have been sent"
    );

    // a valid destination passes validation: the button flips to busy (the
    // net channel has no worker in the probe, so the result never settles)
    dialog_click(cx, "clone-dest-input");
    cx.simulate_input("/tmp");
    cx.run_until_parked();
    dialog_draw(cx);
    dialog_click(cx, "btn-accent-Clone");
    assert!(
        cx.debug_bounds("btn-accent-Cloning…").is_some(),
        "validation passed and the commit went out"
    );
    assert!(cx.debug_bounds("clone-overlay").is_some());
}

/// The kit's header sort glyphs and click cycle read the cached column
/// groups, so the delegate must hand back the store's live sort state: the
/// active column carries the direction, everything else stays unsorted.
#[gpui::test]
fn header_columns_mirror_the_active_sort(cx: &mut TestAppContext) {
    use devtools_core::settings::RepoSortMode;
    use gpui_kit::component::table::ColumnSort;

    cx.update(gpui_kit::init);

    let store = fake_store(cx);
    cx.update_entity(&store, |s: &mut Store, _| {
        // in-memory only: the probe must not rewrite the real settings file
        s.sort_mode = RepoSortMode::LastActivity;
        s.sort_ascending = false;
    });

    cx.update(|cx| {
        let available = gpui::px(1100.);
        let delegate = devtools::ui::repos::ReposDelegate::new(store.clone(), available);
        use gpui_kit::component::table::TableDelegate as _;

        // columns: star, name, shortcuts, branch, changes, …, last activity
        let activity_ix = delegate.columns_count(cx) - 1;
        assert!(
            matches!(delegate.column(activity_ix, cx).sort, Some(ColumnSort::Descending)),
            "the active column shows its direction"
        );
        assert!(
            matches!(delegate.column(1, cx).sort, Some(ColumnSort::Default)),
            "inactive columns stay unsorted"
        );
    });

    cx.update_entity(&store, |s: &mut Store, _| {
        s.sort_mode = RepoSortMode::Name;
        s.sort_ascending = true;
    });
    cx.update(|cx| {
        let delegate = devtools::ui::repos::ReposDelegate::new(store.clone(), gpui::px(1100.));
        use gpui_kit::component::table::TableDelegate as _;

        assert!(
            matches!(delegate.column(1, cx).sort, Some(ColumnSort::Ascending)),
            "switching modes hands the arrow to the new column"
        );
        let activity_ix = delegate.columns_count(cx) - 1;
        assert!(
            matches!(delegate.column(activity_ix, cx).sort, Some(ColumnSort::Default)),
            "the previous column drops back to unsorted"
        );
    });
}
