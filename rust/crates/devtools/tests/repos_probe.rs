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
    cx.update(|cx| devtools::theme::apply_kit_theme(cx));

    let store = fake_store(cx);
    let store_handle = store.clone();
    let (_window_handle, cx) = cx.add_window_view(|window, cx| {
        MainWindow::new(store_handle.clone(), window, cx)
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));

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

#[gpui::test]
fn selection_survives_and_closes_via_the_store(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(|cx| devtools::theme::apply_kit_theme(cx));

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

fn px_col() -> gpui::Pixels {
    gpui::px(120.)
}

fn px_row() -> gpui::Pixels {
    // titlebar 38 + toolbar ~42 + totals ~36 + table head ~30 + row center ~22
    gpui::px(168.)
}
