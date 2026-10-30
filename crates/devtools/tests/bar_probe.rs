//! Headless probes for the bar-opening rules: a row press opens the bar on
//! Overview, a second press on the highlighted row closes it, chip routing
//! lands on a specific tab without toggling, and the notes-button path never
//! closes the bar.

use devtools::store::{BottomBarTab, Store};
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::TestAppContext;
use std::path::PathBuf;

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
        git_modified_count: 3,
        git_to_push_count: 1,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: true,
        git_branch_published: true,
        git_status_loaded: true,
        github_pr_count: 2,
        github_issue_count: 0,
    }
}

fn fake_store(cx: &mut TestAppContext) -> gpui::Entity<Store> {
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        // pin the flat layout: the developer's live settings must never
        // re-shape the probes' row math (the grouping probes seed their own
        // scratch-settings store)
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")]);
        store
    })
}

#[gpui::test]
fn row_press_opens_overview_and_repress_closes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = fake_store(cx);

    cx.update(|cx| {
        store.update(cx, |s, cx| s.select_repo(Some("/probe/alpha".into()), cx));
        assert_eq!(s_tab(&store, cx), BottomBarTab::Overview);
        assert_eq!(s_selected(&store, cx).as_deref(), Some("/probe/alpha"));

        // same row pressed again → the bar closes entirely
        store.update(cx, |s, cx| s.select_repo(Some("/probe/alpha".into()), cx));
        assert_eq!(s_selected(&store, cx), None);

        // a different row reopens on Overview
        store.update(cx, |s, cx| s.select_repo(Some("/probe/beta".into()), cx));
        assert_eq!(s_selected(&store, cx).as_deref(), Some("/probe/beta"));
        assert_eq!(s_tab(&store, cx), BottomBarTab::Overview);
    });
}

#[gpui::test]
fn chip_routing_lands_on_the_requested_tab(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = fake_store(cx);

    cx.update(|cx| {
        store.update(cx, |s, cx| s.open_repo_tab("/probe/alpha", BottomBarTab::PullRequests, cx));
        assert_eq!(s_selected(&store, cx).as_deref(), Some("/probe/alpha"));
        assert_eq!(s_tab(&store, cx), BottomBarTab::PullRequests);

        // routing to the same repo's other tab never closes the bar
        store.update(cx, |s, cx| s.open_repo_tab("/probe/alpha", BottomBarTab::Azure, cx));
        assert_eq!(s_selected(&store, cx).as_deref(), Some("/probe/alpha"));
        assert_eq!(s_tab(&store, cx), BottomBarTab::Azure);

        // routing for a not-selected repo switches and keeps the requested tab
        store.update(cx, |s, cx| s.open_repo_tab("/probe/beta", BottomBarTab::Issues, cx));
        assert_eq!(s_selected(&store, cx).as_deref(), Some("/probe/beta"));
        assert_eq!(s_tab(&store, cx), BottomBarTab::Issues);
    });
}

/// The row's notes icon no longer routes through the store (it launches the
/// external warp-notes app directly), so `ensure_repo_selected` — its old
/// re-fire path — only has to stay selection-stable.
#[gpui::test]
fn ensure_repo_selected_is_stable_for_the_selected_repo(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let store = fake_store(cx);

    cx.update(|cx| {
        store.update(cx, |s, cx| s.select_repo(Some("/probe/alpha".into()), cx));
        store.update(cx, |s, cx| s.ensure_repo_selected("/probe/alpha", cx));
        assert_eq!(s_selected(&store, cx).as_deref(), Some("/probe/alpha"));
    });
}

/// The bar kebab's "Archive repository" item: one click tags the selected
/// repo, its row leaves the list, the bar closes with it and a toast
/// confirms.
#[gpui::test]
fn kebab_archive_repository_tags_the_selected_repo(cx: &mut TestAppContext) {
    use devtools::ui::window::MainWindow;

    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let store = fake_store(cx);
    cx.update_entity(&store, |s, cx| s.select_repo(Some("/probe/alpha".into()), cx));
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(cx.debug_bounds("bar-root").is_some(), "the bar starts open");

    let kebab = cx.debug_bounds("bar-kebab").expect("the kebab button renders");
    cx.simulate_click(kebab.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    let item = cx
        .debug_bounds("kebab-Archive repository")
        .expect("the archive item renders in the kebab menu");
    cx.simulate_click(item.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(
        cx.read_entity(&store, |s, _| s
            .repos
            .iter()
            .find(|r| r.folder_path == std::path::Path::new("/probe/alpha"))
            .is_some_and(|r| r.is_archived())),
        "the kebab item archived the selected repo"
    );
    assert_eq!(
        cx.read_entity(&store, |s, _| s.projected.len()),
        1,
        "the archived row left the list"
    );
    assert!(
        cx.read_entity(&store, |s, _| s.selected_repo.is_none()),
        "the bar closed with the archived repo"
    );
    assert!(
        cx.debug_bounds("toast-0").is_some(),
        "archiving confirms with a toast"
    );
}

fn s_selected(store: &gpui::Entity<Store>, cx: &gpui::App) -> Option<String> {
    store.read(cx).selected_repo.clone()
}

fn s_tab(store: &gpui::Entity<Store>, cx: &gpui::App) -> BottomBarTab {
    store.read(cx).selected_tab
}
