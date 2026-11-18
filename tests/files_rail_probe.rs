//! The pull request's Files rail: the changed files as a tree. A folder folds
//! with the mouse and the keys, folding never moves the file whose diff the
//! pane shows, and the rows scroll when the tree is taller than the rail.

use gpui::prelude::*;
use gpui::{
    Context, Entity, Modifiers, Pixels, Point, Render, ScrollDelta, ScrollEvent, ScrollSource,
    TestAppContext, TouchPhase, VisualTestContext, Window, div, point, px,
};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use warp_station::store::{OpenPull, Store};
use warp_station::ui::pull_request::PullRequestView;
use warp_station_core::github::{GitHubFilePatch, GitHubItem, GitHubPull};
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::SettingsService;
use warpui_kit_core::pane_nav;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "warp-station-files-rail-{name}-{}",
        std::process::id()
    ));
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

struct Host {
    view: Entity<PullRequestView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        // the view's pane is `flex_1`: without a flex parent it would size to
        // its content and no rail could ever be taller than its scroller
        div().size_full().flex().flex_col().child(self.view.clone())
    }
}

fn draw(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.update(|window, cx| drop(window.draw(cx)));
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = vcx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} was not drawn"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    draw(vcx);
}

fn wheel(vcx: &mut VisualTestContext, at: Point<Pixels>, delta: ScrollDelta) {
    vcx.simulate_event(ScrollEvent {
        position: at,
        delta,
        modifiers: Modifiers::default(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    draw(vcx);
}

/// The view on pull request #7 with `files` changed, its Files tab picked.
fn open_files_view<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    files: Vec<GitHubFilePatch>,
) -> &'a mut VisualTestContext {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let settings = scratch(name);
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, Arc::new(SettingsService::at(settings)));
        store.replace_repos(vec![RepoEntry {
            name: "alpha".into(),
            folder_path: PathBuf::from("/probe/alpha"),
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
        }]);
        store.select_repo("/probe/alpha", cx);
        store
    });
    store.update(cx, |store, _| {
        store.open_pull = Some(OpenPull {
            folder: "/probe/alpha".into(),
            item: GitHubItem {
                number: 7,
                title: "Tree the rail".into(),
                is_pull_request: true,
                ..Default::default()
            },
            pull: Some(GitHubPull {
                number: 7,
                files: files.clone(),
                ..Default::default()
            }),
            failed: false,
            merging: false,
            merge_armed: false,
        });
        store.pull_diff.insert(7, Some(files));
    });

    let (_host, vcx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            PullRequestView::new(store.clone(), Rc::new(pane_nav::PaneNav::new()), window, cx)
        });
        Host { view }
    });
    draw(vcx);
    click(vcx, "pull-tab-files");
    vcx
}

#[gpui_kit::gpui::test]
fn the_files_rail_folds_its_tree(cx: &mut TestAppContext) {
    let vcx = open_files_view(
        cx,
        "tree",
        vec![patch("src/a.rs"), patch("src/ui/b.rs"), patch("Cargo.toml")],
    );

    // the rail draws the tree, and the selected file's diff is beside it
    assert!(
        vcx.debug_bounds("pull-folder-src").is_some(),
        "the rail groups the changed files under their folders"
    );
    assert!(
        vcx.debug_bounds("pull-folder-src/ui").is_some(),
        "a nested folder keeps its own row"
    );
    assert!(vcx.debug_bounds("pull-file-0").is_some());
    assert!(
        vcx.debug_bounds("pull-diff-src/a.rs").is_some(),
        "the selected file's diff draws in the pane"
    );

    // a press on a folder folds it away and leaves the diff alone
    click(vcx, "pull-folder-src");
    assert!(
        vcx.debug_bounds("pull-file-0").is_none(),
        "the fold hides the folder's files"
    );
    assert!(
        vcx.debug_bounds("pull-folder-src/ui").is_none(),
        "and the nested folder"
    );
    assert!(
        vcx.debug_bounds("pull-diff-src/a.rs").is_some(),
        "the diff keeps the file it was showing"
    );

    // pressing it again unfolds the subtree
    click(vcx, "pull-folder-src");
    assert!(
        vcx.debug_bounds("pull-file-0").is_some(),
        "the unfold brings the files back"
    );
}

/// A tree taller than the rail scrolls its rows: the scroller takes the rail's
/// height instead of growing past it and being clipped at the tab's edge.
#[gpui_kit::gpui::test]
fn the_files_rail_scrolls_to_its_last_file(cx: &mut TestAppContext) {
    let files: Vec<GitHubFilePatch> = (0..40)
        .map(|index| patch(&format!("src/file_{index:02}.rs")))
        .collect();
    let vcx = open_files_view(cx, "scroll", files);

    let rows = vcx
        .debug_bounds("pull-files")
        .expect("the rail's rows draw");
    let first = vcx
        .debug_bounds("pull-file-0")
        .expect("the first file row draws");
    let last = vcx
        .debug_bounds("pull-file-39")
        .expect("the last file row is built");
    assert!(
        f32::from(rows.size.height) < f32::from(last.origin.y + last.size.height - rows.origin.y),
        "the rows are taller than the rail: rows {rows:?}, last {last:?}"
    );
    assert!(
        first.origin.y >= rows.origin.y,
        "the first row starts inside the rail"
    );

    wheel(
        vcx,
        rows.center(),
        ScrollDelta::Pixels(point(px(0.), px(-1200.))),
    );
    let last = vcx
        .debug_bounds("pull-file-39")
        .expect("the last file row stays built");
    assert!(
        last.origin.y < rows.bottom(),
        "the wheel brings the last file into the rail: rows {rows:?}, last {last:?}"
    );
    let first = vcx
        .debug_bounds("pull-file-0")
        .expect("the first row stays built");
    assert!(
        first.origin.y < rows.origin.y,
        "the first row scrolled above the rail: {first:?}"
    );
}
