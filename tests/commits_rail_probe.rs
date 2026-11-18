//! The pull request's Commits tab: the day-grouped commit list and the selected
//! commit's own file rail. Each is its own list, so the keys move the list that
//! holds the focus and never its neighbour.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use warp_station::store::{Store, WorkspaceTab};
use warp_station::ui::window::MainWindow;
use warp_station_core::github::{
    GitHubCommit, GitHubFilePatch, GitHubItem, GitHubPull, GitHubSnapshot,
};
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::SettingsService;

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

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "warp-station-commits-rail-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, "{}").unwrap();
    path
}

fn commit(index: usize) -> GitHubCommit {
    GitHubCommit {
        oid: format!("{:040x}", index + 1),
        headline: format!("commit {index}"),
        author: Some("Probe Author".into()),
        committed_at: Some("2026-07-27T10:00:00Z".into()),
    }
}

fn patch(path: &str) -> GitHubFilePatch {
    GitHubFilePatch {
        path: path.into(),
        status: "M".into(),
        additions: 1,
        deletions: 0,
        patch: format!("--- a/{path}\n+++ b/{path}\n@@ -1,1 +1,1 @@\n-a\n+b\n"),
    }
}

fn pull(number: u64) -> GitHubItem {
    GitHubItem {
        number,
        title: format!("PR {number}"),
        url: format!("https://example.test/pull/{number}"),
        author: Some("Probe Author".into()),
        is_pull_request: true,
        state: Some("OPEN".into()),
        head_ref_name: Some("feature".into()),
        base_ref_name: Some("main".into()),
        ..Default::default()
    }
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} renders"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    draw(cx);
}

/// The workbench on pull request #1, its Commits tab up. `files` is the
/// selected commit's own file list, per commit, exactly as the net thread
/// answers `GitHubPullCommitFiles`.
fn open_commits_window<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    commits: Vec<GitHubCommit>,
    files: Vec<Vec<GitHubFilePatch>>,
) -> &'a mut gpui::VisualTestContext {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(SettingsService::at(scratch(name))),
        );
        store.group_by_root = false;
        store.pane_width = 1400.;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store.github_by_folder.insert(
            "/probe/alpha".into(),
            std::sync::Arc::new(GitHubSnapshot {
                url: Some("https://github.com/probe/alpha".into()),
                pulls: vec![pull(1)],
                issues: Vec::new(),
                runs: Vec::new(),
                available: true,
            }),
        );
        store.select_repo("/probe/alpha", _cx);
        store.open_detail_tab(WorkspaceTab::PullRequests, _cx);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);
    click(cx, "PR-gh-1");
    cx.update_entity(&store, |store, cx| {
        let open = store.open_pull.as_mut().expect("the row opened");
        open.pull = Some(GitHubPull {
            number: 1,
            commits: commits.clone(),
            ..Default::default()
        });
        for (commit, files) in commits.iter().zip(files) {
            store
                .pull_commit_files
                .insert(commit.oid.clone(), Some(files));
        }
        cx.notify();
    });
    draw(cx);
    click(cx, "pull-tab-commits");
    cx
}

/// `↓`/`↑`/`Home`/`End` on the focused commit list walk it: the selected
/// commit's head says which row is the selection.
#[gpui::test]
fn the_commit_list_walks_with_the_keys(cx: &mut TestAppContext) {
    let commits = vec![commit(0), commit(1), commit(2)];
    let files = vec![
        vec![patch("a.rs")],
        vec![patch("b.rs")],
        vec![patch("c.rs")],
    ];
    let cx = open_commits_window(cx, "keys", commits.clone(), files);
    assert!(cx.debug_bounds("pull-commits").is_some(), "the list draws");
    assert!(cx.debug_bounds("pull-commit-0").is_some());

    // a press on a row focuses the list; the detail head names the row
    click(cx, "pull-commit-0");
    assert!(cx.debug_bounds("pull-commit-head-0").is_some());

    cx.simulate_keystrokes("down");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-head-1").is_some(),
        "↓ walks to the second commit"
    );

    cx.simulate_keystrokes("down");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-head-2").is_some(),
        "↓ walks to the third commit"
    );

    cx.simulate_keystrokes("up");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-head-1").is_some(),
        "↑ walks back to the second commit"
    );

    cx.simulate_keystrokes("home");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-head-0").is_some(),
        "Home goes to the first commit"
    );

    cx.simulate_keystrokes("end");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-head-2").is_some(),
        "End goes to the last commit"
    );
}

/// The selected commit's files are their own list: with the rail focused the
/// arrows step the file, and the commit list keeps its row.
#[gpui::test]
fn the_commit_files_rail_walks_with_the_keys(cx: &mut TestAppContext) {
    let commits = vec![commit(0), commit(1)];
    let files = vec![vec![patch("a.rs"), patch("b.rs")], vec![patch("c.rs")]];
    let cx = open_commits_window(cx, "files", commits, files);
    click(cx, "pull-commit-0");
    assert!(
        cx.debug_bounds("pull-commit-diff-a.rs").is_some(),
        "the commit's first file draws beside the list"
    );

    // a press on a file row focuses the rail; the rail's cursor steps the
    // commit's files and its diff header names the one it lands on
    click(cx, "pull-commit-file-0");
    cx.simulate_keystrokes("down");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-diff-b.rs").is_some(),
        "↓ steps to the second file of the commit"
    );
    assert!(
        cx.debug_bounds("pull-commit-diff-a.rs").is_none(),
        "the diff follows the rail's cursor"
    );
    assert!(
        cx.debug_bounds("pull-commit-head-0").is_some(),
        "a step in the files rail leaves the commit list alone"
    );

    cx.simulate_keystrokes("home");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-diff-a.rs").is_some(),
        "Home goes back to the commit's first file"
    );

    // ← hands the keys to the commit list; the commit rail's own step still
    // lands on its file
    cx.simulate_keystrokes("left");
    cx.simulate_keystrokes("down");
    draw(cx);
    assert!(
        cx.debug_bounds("pull-commit-head-1").is_some(),
        "← focuses the commit list and ↓ walks it"
    );
    assert!(
        cx.debug_bounds("pull-commit-diff-c.rs").is_some(),
        "the second commit's own file draws"
    );
}
