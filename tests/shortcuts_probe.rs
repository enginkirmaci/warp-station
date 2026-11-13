//! The keyboard sheet: `?`/Ctrl+/ and the titlebar entry open it, it owns the
//! keys while it is up, and every close path gives the keyboard back to the
//! window root.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;
use warp_station::store::Store;
use warp_station::ui::window::MainWindow;
use warp_station_core::repo_store::RepoEntry;

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

fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-shortcuts-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (gpui::Entity<Store>, &'a mut gpui::VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(net_tx, scratch_service(name));
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, vcx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(vcx);
    (store, vcx)
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
}

fn sheet(cx: &mut gpui::VisualTestContext) -> bool {
    cx.debug_bounds("shortcut-sheet-card").is_some()
}

#[gpui::test]
fn question_mark_opens_the_sheet_and_escape_closes_it(cx: &mut TestAppContext) {
    let (store, cx) = open(cx, "question-open");

    cx.simulate_keystrokes("?");
    draw(cx);
    assert!(sheet(cx), "? opens the sheet");
    assert!(
        cx.debug_bounds("shortcut-sheet-list").is_some(),
        "the key rows draw"
    );

    // the card owns the keys: ↓ must not move the list under it
    let before = cx.read_entity(&store, |s, _| s.selected_repo.clone());
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.selected_repo.clone()),
        before,
        "↓ under the sheet moves nothing"
    );

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!sheet(cx), "Escape closes it");

    // closing hands the keyboard back to the window root, which delegates
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(
        cx.read_entity(&store, |s, _| s.selected_repo.clone())
            .as_deref(),
        Some("/probe/beta"),
        "↓ moves the repo list again"
    );
}

#[gpui::test]
fn ctrl_slash_and_the_titlebar_button_open_the_sheet(cx: &mut TestAppContext) {
    let (_store, cx) = open(cx, "entry-points");

    cx.simulate_keystrokes("ctrl-/");
    draw(cx);
    assert!(sheet(cx), "Ctrl+/ opens it");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!sheet(cx));

    let button = cx
        .debug_bounds("shortcuts-toggle")
        .expect("the titlebar has the entry");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(sheet(cx), "the titlebar button opens it");

    let close = cx
        .debug_bounds("shortcut-sheet-close")
        .expect("the card has a close button");
    cx.simulate_click(close.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(!sheet(cx), "the close button closes it");
}

#[gpui::test]
fn a_focused_field_keeps_the_question_mark(cx: &mut TestAppContext) {
    let (_store, cx) = open(cx, "field-guard");

    cx.simulate_keystrokes("ctrl-k");
    draw(cx);
    cx.simulate_keystrokes("?");
    draw(cx);
    assert!(
        !sheet(cx),
        "typing in the search field never opens the sheet"
    );
}
