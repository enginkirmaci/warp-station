//! Headless draw probe for the Changes workspace: a real window over a store
//! loaded with the live repository's actual snapshot, branches and history —
//! two draws without a panic means the view's element tree is sound.

use devtools::store::Store;
use devtools::ui::window::MainWindow;
use gpui::prelude::*;
use gpui::TestAppContext;

#[gpui_kit::gpui::test]
fn changes_view_draws_over_real_data(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| devtools::theme::apply_kit_theme(cx));

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
