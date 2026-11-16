//! The pull request's Files rail: the changed files as a tree. A folder folds
//! with the mouse and the keys, and folding never moves the file whose diff
//! the pane shows.

use gpui::prelude::*;
use gpui::{Context, Entity, Modifiers, Render, TestAppContext, VisualTestContext, Window, div};
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
        div().size_full().child(self.view.clone())
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

#[gpui_kit::gpui::test]
fn the_files_rail_folds_its_tree(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let settings = scratch("tree");
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
    let files = vec![patch("src/a.rs"), patch("src/ui/b.rs"), patch("Cargo.toml")];
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
