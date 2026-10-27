//! Headless draw probe for the Changes workspace: a real window over a store
//! loaded with the live repository's actual snapshot, branches and history —
//! two draws without a panic means the view's element tree is sound.

use devtools::store::Store;
use devtools::ui::window::MainWindow;
use gpui::prelude::*;
use gpui::{point, px, Modifiers, TestAppContext, VisualTestContext};

#[gpui_kit::gpui::test]
fn changes_view_draws_over_real_data(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let here = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        // prime the bottom bar with a REAL snapshot of this repository
        let snapshot = devtools_core::git::read::change_snapshot(&here);
        let branches = devtools_core::git::read::branches(&here).unwrap_or_default();
        let recent = devtools_core::git::read::recent_commits(&here).unwrap_or_default();
        let unpushed = devtools_core::git::read::unpushed_hashes(&here);
        store.replace_repos(vec![devtools_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: here.clone(),
            solution_path: None,
            tags: vec![],
            git_branch_name: Some("main".into()),
            git_modified_count: snapshot
                .as_ref()
                .map(|s| s.status_files.len() as u32)
                .unwrap_or(0),
            git_to_push_count: 0,
            git_to_pull_count: 0,
            git_last_commit_at: None,
            git_last_fetch_at: None,
            git_has_remote: true,
            git_branch_published: true,
            git_status_loaded: true,
            github_pr_count: 0,
            github_issue_count: 0,
        }]);
        store.select_repo(Some(here.to_string_lossy().to_string()), cx);
        store.changes = snapshot;
        store.changes_loaded_for = Some(here.to_string_lossy().to_string());
        store.branches = branches;
        store.recent_commits = recent;
        store.unpushed = unpushed;
        store
    });

    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    // a second draw after an expand toggle would need input; two consecutive
    // draws still exercise the full tree including the rail and both sections
    cx.update(|window, cx| drop(window.draw(cx)));

    let loaded = cx.read_entity(&store, |s, _| s.changes_loaded_for.is_some());
    assert!(loaded, "changes should be loaded for the selected repo");
}

#[gpui_kit::gpui::test]
fn history_and_drill_down_draw(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let here = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .to_path_buf();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.replace_repos(vec![devtools_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: here.clone(),
            solution_path: None,
            tags: vec![],
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
        }]);
        store.select_repo(Some(here.to_string_lossy().to_string()), cx);
        store.changes = devtools_core::git::read::change_snapshot(&here);
        store.changes_loaded_for = Some(here.to_string_lossy().to_string());
        store.history = devtools_core::git::read::history_commits(&here, 30).unwrap_or_default();
        store.unpushed = devtools_core::git::read::unpushed_hashes(&here);
        store
    });
    assert!(
        cx.read_entity(&store, |s, _| s.history.len()) > 5,
        "probe repo should have history"
    );

    let (view_handle, cx) = cx.add_window_view(|window, cx| {
        devtools::ui::changes::ChangesView::new(store.clone(), window, cx)
    });
    cx.run_until_parked();

    // list pane
    view_handle.update(cx, |view, cx| view.open_history(cx));
    cx.run_until_parked();
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    // drill into the newest commit (details pre-seeded the way the net would)
    let head_hash = cx.read_entity(&store, |s, _| {
        s.history.first().map(|c| c.hash.clone()).unwrap_or_default()
    });
    let details = devtools_core::git::read::commit_details(&here, &head_hash);
    view_handle.update(cx, |view, cx| view.drill_commit(&head_hash, cx));
    store.update(cx, |s: &mut Store, _| {
        s.commit_details = details;
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} missing from the frame"));
    cx.simulate_click(
        gpui::Point {
            x: b.origin.x + b.size.width / 2.,
            y: b.origin.y + b.size.height / 2.,
        },
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn git_commands(net_rx: &async_channel::Receiver<devtools::net::NetCommand>) -> Vec<devtools::net::NetCommand> {
    let mut git = Vec::new();
    while let Ok(cmd) = net_rx.try_recv() {
        if matches!(cmd, devtools::net::NetCommand::Git { .. }) {
            git.push(cmd);
        }
    }
    git
}

#[gpui_kit::gpui::test]
fn branch_delete_gates_on_kind_current_and_two_clicks(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    // the receiver stays bound: the confirmed delete must arrive on it
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.replace_repos(vec![devtools_core::repo_store::RepoEntry {
            name: "probe-repo".into(),
            folder_path: "/probe/alpha".into(),
            solution_path: None,
            tags: vec![],
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
        }]);
        store.select_repo(Some("/probe/alpha".into()), cx);
        store.selected_tab = devtools::store::BottomBarTab::Changes;
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.branches = vec![
            devtools_core::model::GitBranchRef {
                name: "main".into(),
                kind: devtools_core::model::GitBranchKind::Local,
            },
            devtools_core::model::GitBranchRef {
                name: "feature/x".into(),
                kind: devtools_core::model::GitBranchKind::Local,
            },
            devtools_core::model::GitBranchRef {
                name: "feature/keep".into(),
                kind: devtools_core::model::GitBranchKind::Local,
            },
            devtools_core::model::GitBranchRef {
                name: "origin/feature/x".into(),
                kind: devtools_core::model::GitBranchKind::Remote,
            },
        ];
        store
    });

    let (_window, cx) =
        cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    click(cx, "branch-button");
    assert!(cx.debug_bounds("branch-menu").is_some(), "the branch menu opens");

    // the trash is offered on deletable locals only
    assert!(
        cx.debug_bounds("branch-delete-main").is_none(),
        "never on the checked-out branch"
    );
    assert!(
        cx.debug_bounds("branch-delete-origin/feature/x").is_none(),
        "never on remote rows"
    );
    assert!(cx.debug_bounds("branch-delete-feature/x").is_some());
    assert!(cx.debug_bounds("branch-delete-feature/keep").is_some());

    // first click arms: the menu stays open and nothing runs
    click(cx, "branch-delete-feature/x");
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "arming keeps the menu open"
    );
    assert!(git_commands(&net_rx).is_empty(), "the arm click must not run git");

    // an outside press closes the menu and disarms — the reopened menu starts
    // from a single-click arm again
    cx.simulate_click(point(px(4.), px(4.)), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "the outside press closed the menu"
    );
    click(cx, "branch-button");
    assert!(cx.debug_bounds("branch-menu").is_some(), "the menu reopens");
    click(cx, "branch-delete-feature/x");
    assert!(
        git_commands(&net_rx).is_empty(),
        "reopening disarmed the trash"
    );

    // arm, then confirm: exactly one delete reaches the net, never a checkout
    click(cx, "branch-delete-feature/x");
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "confirming closes the menu"
    );
    let sent = git_commands(&net_rx);
    assert_eq!(sent.len(), 1, "one git command — the delete");
    let devtools::net::NetCommand::Git { folder, action } = &sent[0] else {
        unreachable!("git_commands only collects Git commands");
    };
    assert_eq!(folder, std::path::Path::new("/probe/alpha"));
    assert!(
        matches!(action, devtools::net::GitAction::DeleteBranch(name) if name == "feature/x"),
        "the delete fires, not a checkout: {action:?}"
    );
    assert!(
        cx.read_entity(&store, |s, _| s.git_busy),
        "run_git marked the rail busy"
    );
}
