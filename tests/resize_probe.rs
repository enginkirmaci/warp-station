//! Every list column has a drag bar on its right edge: the work list, the PR
//! Files rail, the commit list and the commit's files rail, plus the page list
//! the Settings section shares. A drag moves the column inside its bounds and
//! the drop keeps the width.

use gpui::prelude::*;
use gpui::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, TestAppContext, point, px};
use std::path::PathBuf;
use warp_station::store::{Pane, Store, WorkspaceTab};
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
    let dir =
        std::env::temp_dir().join(format!("warp-station-resize-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, "{}").unwrap();
    path
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

fn commit(index: usize) -> GitHubCommit {
    GitHubCommit {
        oid: format!("{:040x}", index + 1),
        headline: format!("commit {index}"),
        author: Some("Probe Author".into()),
        committed_at: Some("2026-07-27T10:00:00Z".into()),
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

/// Drags `handle` along the x axis, ending at `to`, the way a pointer drag on
/// the bar arrives: a press, a move past the drag threshold, then the drop.
fn drag_bar(cx: &mut gpui::VisualTestContext, handle: &'static str, to: f32) {
    let start = cx
        .debug_bounds(handle)
        .unwrap_or_else(|| panic!("{handle} draws"))
        .center();
    cx.simulate_event(MouseDownEvent {
        position: start,
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    let target = point(px(to), start.y);
    for position in [point(start.x + px(8.), start.y), target, target] {
        cx.simulate_event(MouseMoveEvent {
            position,
            pressed_button: Some(MouseButton::Left),
            modifiers: gpui::Modifiers::default(),
        });
    }
    cx.simulate_event(MouseUpEvent {
        position: target,
        button: MouseButton::Left,
        modifiers: gpui::Modifiers::default(),
        click_count: 1,
    });
    cx.run_until_parked();
    draw(cx);
}

/// The workbench on pull request #1 with its Commits tab up and both the
/// commit and its files seeded, so every rail in the tab draws.
fn open_workbench<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (&'a mut gpui::VisualTestContext, gpui::Entity<Store>) {
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
        let commits = vec![commit(0), commit(1)];
        let files = vec![patch("src/a.rs"), patch("src/b.rs")];
        let open = store.open_pull.as_mut().expect("the row opened");
        open.pull = Some(GitHubPull {
            number: 1,
            files: files.clone(),
            commits: commits.clone(),
            ..Default::default()
        });
        store.pull_diff.insert(1, Some(files));
        for commit in commits {
            store
                .pull_commit_files
                .insert(commit.oid, Some(vec![patch("src/a.rs"), patch("src/b.rs")]));
        }
        cx.notify();
    });
    draw(cx);
    (cx, store)
}

/// The work list's bar moves the column, and the drop keeps the width.
#[gpui::test]
fn the_work_list_edge_drags_inside_its_bounds(cx: &mut TestAppContext) {
    let (cx, store) = open_workbench(cx, "work");
    let list = cx.debug_bounds("pr-list-column").expect("the work list");
    drag_bar(cx, "resize-work-list", f32::from(list.left()) + 400.);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.pane_width(Pane::WorkList)),
        400.
    );
    assert_eq!(
        cx.read_entity(&store, |s, _| s.settings().layout.pane_widths["work_list"]),
        400.,
        "the drop saved the width"
    );
    let moved = cx.debug_bounds("pr-list-column").unwrap();
    assert_eq!(f32::from(moved.size.width), 400.);

    // past the widest bound the column stops at it
    drag_bar(cx, "resize-work-list", f32::from(moved.left()) + 2000.);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.pane_width(Pane::WorkList)),
        cx.read_entity(&store, |_, _| Pane::WorkList.clamp(2000.))
    );
}

/// The PR Files rail and the Commits tab's two cols drag on their own edges.
#[gpui::test]
fn the_detail_rails_drag_on_their_own_edges(cx: &mut TestAppContext) {
    let (cx, store) = open_workbench(cx, "rails");

    // the Files tab's rail
    click(cx, "pull-tab-files");
    let rail = cx.debug_bounds("pull-files").expect("the files rail");
    drag_bar(cx, "resize-files-rail", f32::from(rail.left()) + 320.);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.pane_width(Pane::FilesRail)),
        320.
    );

    // the Commits tab's commit list
    click(cx, "pull-tab-commits");
    let commits = cx.debug_bounds("pull-commits").expect("the commit list");
    drag_bar(cx, "resize-commits-list", f32::from(commits.left()) + 300.);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.pane_width(Pane::CommitsList)),
        300.
    );
    // the column draws its 1px hairline inside the 300 it was dragged to
    assert_eq!(
        f32::from(cx.debug_bounds("pull-commits").unwrap().size.width),
        299.
    );

    // the commit's own files, inside the commit's detail
    let files = cx
        .debug_bounds("pull-commit-files")
        .expect("the commit's files rail");
    drag_bar(cx, "resize-commit-files", f32::from(files.left()) + 280.);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.pane_width(Pane::CommitFiles)),
        280.
    );
}

/// The page lists share one width and one bar: dragging the Settings list
/// moves the width the repo list also reads, and the drop saves it.
#[gpui::test]
fn the_page_list_edge_drags_on_every_page(cx: &mut TestAppContext) {
    let (cx, store) = open_workbench(cx, "pages");
    // the rail's Settings entry opens the page; its rows land a frame after
    click(cx, "rail-settings");
    draw(cx);
    let detail = cx
        .debug_bounds("settings-detail")
        .expect("the Settings detail draws");
    let width = cx.read_entity(&store, |s, _| s.list_width());
    // the list's left edge is the detail's left edge less its own width
    drag_bar(cx, "list-resize", f32::from(detail.left()) - width + 390.);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.list_width()),
        390.,
        "the page list reads the shared width"
    );
    assert_eq!(
        cx.read_entity(&store, |s, _| s.settings().repos.list_width),
        Some(390.),
        "the drop saved it where the repo list already reads it"
    );
    let moved = cx.debug_bounds("settings-detail").unwrap();
    assert_eq!(
        f32::from(moved.left()),
        f32::from(detail.left()) - width + 390.
    );
}

/// A column that is stacked above its detail has nothing to drag against, so
/// it draws no bar; the work list beside it keeps both columns and its bar.
#[gpui::test]
fn a_stacked_column_has_no_bar(cx: &mut TestAppContext) {
    let (cx, _store) = open_workbench(cx, "stacked");
    cx.simulate_resize(gpui::size(gpui::px(1500.), gpui::px(800.)));
    draw(cx);
    click(cx, "pull-tab-files");
    assert!(
        cx.debug_bounds("pull-files-band").is_some(),
        "a narrow detail stacks the Files rail above its diff"
    );
    assert!(
        cx.debug_bounds("resize-files-rail").is_none(),
        "a stacked rail has no bar"
    );
    assert!(
        cx.debug_bounds("resize-work-list").is_some(),
        "the work list still has both columns and keeps its bar"
    );
}
