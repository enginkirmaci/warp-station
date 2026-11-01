//! The split main screen's detail rail must dispatch, not just draw: the star
//! toggles the repo's favourite flag and the sync verbs reach the net thread.
//! The table probe (repos_probe) pins `DEVTOOLS_UI_TABLE=1`; this one stays on
//! the default split layout, which is what the app ships.

use devtools::net::{GitAction, NetCommand};
use devtools::store::Store;
use devtools::ui::window::MainWindow;
use devtools_core::repo_store::RepoEntry;
use gpui::TestAppContext;
use gpui::prelude::*;
use std::path::PathBuf;

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: PathBuf::from(path),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
        git_modified_count: 3,
        git_to_push_count: 2,
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

fn selected(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) -> Option<String> {
    cx.read_entity(store, |s, _| s.selected_repo.clone())
}

fn load_changes_folders(net_rx: &async_channel::Receiver<NetCommand>) -> Vec<String> {
    let mut folders = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::LoadChanges { folder } = command {
            folders.push(folder.to_string_lossy().to_string());
        }
    }
    folders
}

#[gpui::test]
fn rail_star_toggles_the_favourite(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let star = cx.debug_bounds("insp-star").expect("the rail star renders");
    cx.simulate_click(star.center(), gpui::Modifiers::default());
    cx.run_until_parked();

    assert!(
        cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "clicking the rail star adds the favourites tag"
    );

    // and the same control takes it back off
    draw(cx);
    let star = cx.debug_bounds("insp-star").expect("the rail star renders");
    cx.simulate_click(star.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(
        !cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "a second click removes it again"
    );
}

#[gpui::test]
fn rail_sync_verbs_reach_the_net_thread(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    for (selector, expected) in [
        ("insp-pull", GitAction::Pull),
        ("insp-push", GitAction::Push),
    ] {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} renders"));
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        cx.run_until_parked();

        let mut seen = false;
        while let Ok(command) = net_rx.try_recv() {
            if let NetCommand::Git { action, .. } = command
                && std::mem::discriminant(&action) == std::mem::discriminant(&expected)
            {
                seen = true;
            }
        }
        assert!(seen, "{selector} dispatches its git action");
        draw(cx);
    }
}

#[gpui::test]
fn keymap_steps_rows_and_favorites(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
            entry("gamma", "/probe/gamma"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    // the first row starts selected, so the first ↓ lands on the second
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&store, |s, _| s.selected_repo.clone())
            .as_deref(),
        Some("/probe/beta"),
        "↓ steps to the next row"
    );

    draw(cx);
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(
        cx.read_entity(&store, |s, _| s.selected_repo.clone())
            .as_deref(),
        Some("/probe/alpha"),
        "↑ steps back"
    );

    draw(cx);
    cx.simulate_keystrokes("s");
    cx.run_until_parked();
    assert!(
        cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "S favorites the focused row"
    );
}

#[gpui::test]
fn commit_key_opens_the_changes_drawer(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert!(
        cx.debug_bounds("drawer-scrim").is_none(),
        "no drawer before the commit action"
    );
    cx.simulate_keystrokes("c");
    cx.run_until_parked();
    assert!(
        cx.read_entity(&store, |s, _| s.changes_requested),
        "the C key raises the request flag"
    );
    draw(cx);
    assert!(
        cx.debug_bounds("drawer-scrim").is_some(),
        "C opens the Changes drawer"
    );
}

#[gpui::test]
fn rail_menu_opens_and_the_tag_chip_switches_to_an_editor(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    // "+ tag" swaps the chip for the inline editor
    assert!(
        cx.debug_bounds("insp-add-tag").is_some(),
        "the add-tag chip renders"
    );
    let chip = cx
        .debug_bounds("insp-add-tag")
        .expect("the add-tag chip renders");
    cx.simulate_click(chip.center(), gpui::Modifiers::default());
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("insp-add-tag").is_none(),
        "the chip is replaced by the tag editor"
    );

    // the "⋯" menu lists the repo verbs
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the menu starts closed"
    );
    let more = cx
        .debug_bounds("insp-more")
        .expect("the more button renders");
    cx.simulate_click(more.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(
        cx.debug_bounds("insp-repo-menu").is_some(),
        "the ⋯ button opens the repo menu"
    );

    // "Archive" tags the repo, which drops it from the live list
    let archive = cx
        .debug_bounds("insp-menu-archive")
        .expect("the archive row renders");
    cx.simulate_click(archive.center(), gpui::Modifiers::default());
    draw(cx);
    assert!(
        cx.read_entity(&store, |s, _| s.repos[0].is_archived()),
        "the menu's Archive row tags the repo"
    );
}

#[gpui::test]
fn scope_segments_filter_the_projected_list(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        let mut clean = entry("beta", "/probe/beta");
        clean.git_modified_count = 0;
        clean.git_to_push_count = 0;
        store.replace_repos(vec![entry("alpha", "/probe/alpha"), clean]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let rows = |cx: &mut gpui::VisualTestContext| cx.read_entity(&store, |s, _| s.projected.len());
    assert_eq!(rows(cx), 2, "both rows projected at the start");

    let attention = cx
        .debug_bounds("scope-attention")
        .expect("the needs-attention segment renders");
    cx.simulate_click(attention.center(), gpui::Modifiers::default());
    draw(cx);
    assert_eq!(
        rows(cx),
        1,
        "Needs attention keeps only the repo with work waiting"
    );

    let all = cx
        .debug_bounds("scope-all")
        .expect("the All segment renders");
    cx.simulate_click(all.center(), gpui::Modifiers::default());
    draw(cx);
    assert_eq!(rows(cx), 2, "All clears the scope again");
}

#[gpui::test]
fn tag_editor_commits_on_enter(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let chip = cx
        .debug_bounds("insp-add-tag")
        .expect("the add-tag chip renders");
    cx.simulate_click(chip.center(), gpui::Modifiers::default());
    draw(cx);
    draw(cx);

    cx.simulate_keystrokes("fresh");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    draw(cx);

    assert!(
        cx.read_entity(&store, |s, _| s.repos[0]
            .tags
            .iter()
            .any(|t| t.eq_ignore_ascii_case("fresh"))),
        "typing a tag and pressing Enter writes it"
    );
}

#[gpui::test]
fn rail_file_rows_render_from_a_snapshot(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store.set_changes(Some(devtools_core::model::GitChangeSnapshot {
            status_files: vec![devtools_core::model::GitChangedFile {
                path: "src/main.rs".into(),
                status_code: "M".into(),
                additions: Some(3),
                deletions: Some(1),
            }],
            staged_files: Vec::new(),
            unstaged_files: Vec::new(),
            worktree_counts: Default::default(),
            cached_counts: Default::default(),
        }));
        store.selected_repo = Some("/probe/alpha".into());
        store.changes_loaded_for = Some("/probe/alpha".into());
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert!(
        cx.debug_bounds("insp-file-src/main.rs").is_some(),
        "the uncommitted file row renders from the snapshot"
    );
}

#[gpui::test]
fn rail_issue_rows_render_from_a_github_snapshot(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        let snapshot = devtools_core::github::GitHubSnapshot {
            available: true,
            issues: vec![devtools_core::github::GitHubItem {
                number: 482,
                title: "Agent stream drops on reconnect".into(),
                url: "https://example.invalid/482".into(),
                author: None,
                labels: Vec::new(),
                is_draft: false,
                review_decision: None,
                updated_at: None,
                head_ref_name: None,
                base_ref_name: None,
                is_pull_request: false,
            }],
            // the snapshot's other fields default: no PRs, no URL
            ..Default::default()
        };
        store
            .github_by_folder
            .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert!(
        cx.debug_bounds("insp-issue-482").is_some(),
        "the issue row renders from the GitHub snapshot"
    );
}

/// The press-again-to-close toggle belongs to the old table's bar. Here the
/// rail stays open, so pressing the selected row must leave it selected, not
/// drop the rail back to the first row.
#[gpui::test]
fn pressing_the_selected_row_keeps_it_selected(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    click(cx, "repo-row-1");
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "a press selects the row"
    );

    click(cx, "repo-row-1");
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "a second press changes nothing"
    );
}

/// ↑↓ clamp at the ends of the list; re-selecting the clamped row used to
/// count as a second press and clear the selection.
#[gpui::test]
fn stepping_past_the_list_ends_keeps_the_selection(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    cx.simulate_keystrokes("down");
    draw(cx);
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "↓ on the last row stays there"
    );

    cx.simulate_keystrokes("up");
    draw(cx);
    cx.simulate_keystrokes("up");
    draw(cx);
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "↑ on the first row stays there"
    );
}

/// The rail's sections load per selection, so the row it opens on has to be a
/// real selection: a rail that only previewed the first row sat on "loading
/// changes…" forever.
#[gpui::test]
fn the_first_row_is_selected_and_loaded_without_a_press(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/alpha"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()],
        "the rail's file list loads for the row it opens on"
    );

    // the selected repo leaving the list hands the rail to the next row
    click(cx, "insp-more");
    click(cx, "insp-menu-archive");
    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/beta"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/beta".to_string()],
        "the replacement row loads too"
    );

    // so does a rescan that no longer finds it
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.replace_repos(vec![entry("gamma", "/probe/gamma")]);
        cx.notify();
    });
    draw(cx);
    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/gamma"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/gamma".to_string()]
    );
}

/// A first launch opens on an empty list: the rail picks up the first row as
/// soon as a scan delivers one.
#[gpui::test]
fn the_first_repo_to_arrive_is_selected(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(Vec::new());
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);
    assert_eq!(selected(&store, cx), None, "nothing to select yet");

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        cx.notify();
    });
    draw(cx);

    assert_eq!(selected(&store, cx).as_deref(), Some("/probe/alpha"));
    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()]
    );
}

fn open_changes_sheet(
    cx: &mut TestAppContext,
) -> (gpui::Entity<Store>, &mut gpui::VisualTestContext) {
    use devtools_core::model::{GitBranchKind, GitBranchRef};

    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store.branches = vec![GitBranchRef {
            name: "main".into(),
            kind: GitBranchKind::Local,
        }];
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    cx.simulate_keystrokes("c");
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("changes-sheet").is_some(), "C opens it");
    (store, cx)
}

/// The scrim sits under the panel, so a panel that lets presses through closes
/// the sheet on every click inside it.
#[gpui::test]
fn presses_inside_the_changes_sheet_keep_it_open(cx: &mut TestAppContext) {
    let (_store, cx) = open_changes_sheet(cx);

    click(cx, "changes-sheet");
    assert!(
        cx.debug_bounds("changes-sheet").is_some(),
        "a press on the panel stays inside it"
    );

    let scrim = cx.debug_bounds("drawer-scrim").expect("the scrim renders");
    cx.simulate_click(
        scrim.origin + gpui::point(gpui::px(40.), gpui::px(120.)),
        gpui::Modifiers::default(),
    );
    draw(cx);
    assert!(
        cx.debug_bounds("changes-sheet").is_none(),
        "a press on the scrim closes the sheet"
    );
}

/// The sheet is a layer of its own, so the branch popup inside it has to close
/// first: Escape and outside presses used to route to the hidden workspace
/// bar's picker, and the sheet's own stayed open across a close and reopen.
#[gpui::test]
fn the_sheet_branch_menu_closes_before_the_sheet(cx: &mut TestAppContext) {
    let (_store, cx) = open_changes_sheet(cx);

    click(cx, "branch-button");
    assert!(
        cx.debug_bounds("branch-menu").is_some(),
        "the dropdown opens"
    );
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "Escape closes the dropdown"
    );
    assert!(
        cx.debug_bounds("changes-sheet").is_some(),
        "the first Escape leaves the sheet open"
    );

    click(cx, "branch-button");
    click(cx, "changes-sheet");
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "a press elsewhere in the sheet closes the dropdown"
    );
    assert!(
        cx.debug_bounds("changes-sheet").is_some(),
        "and leaves the sheet open"
    );

    click(cx, "branch-button");
    click(cx, "changes-close");
    assert!(
        cx.debug_bounds("changes-sheet").is_none(),
        "the close button closes the sheet"
    );
    cx.simulate_keystrokes("c");
    draw(cx);
    draw(cx);
    assert!(
        cx.debug_bounds("changes-sheet").is_some(),
        "the window's keys survive the dropdown's filter leaving the tree"
    );
    assert!(
        cx.debug_bounds("branch-menu").is_none(),
        "a reopened sheet starts with its dropdown closed"
    );

    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("changes-sheet").is_none(),
        "Escape closes the sheet once its dropdown is gone"
    );

    // the dropdown's filter held the focus: the next Escape must still land
    cx.simulate_keystrokes("c");
    draw(cx);
    draw(cx);
    click(cx, "branch-button");
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(
        cx.debug_bounds("changes-sheet").is_none(),
        "two Escapes close the dropdown, then the sheet"
    );
}

/// A dialog is a layer on top of the split screen, not a layout switch:
/// opening Add repo used to skip the split branch entirely and drop the
/// legacy data table back in behind the card.
#[gpui::test]
fn dialogs_render_over_the_split_layout(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::new(cx, net_tx);
        store.group_by_root = false;
        store.replace_repos(vec![entry("alpha", "/probe/alpha")]);
        store
    });
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);

    let button = cx
        .debug_bounds("add-repo-button")
        .expect("the toolbar offers Add repo");
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    draw(cx);

    assert!(
        cx.debug_bounds("add-repo-overlay").is_some(),
        "the dialog opens"
    );
    assert!(
        cx.debug_bounds("repos-inspector").is_some(),
        "the detail rail stays behind the dialog"
    );
    assert!(
        cx.debug_bounds("repos-table-wrap").is_none(),
        "the legacy table must not come back while a dialog is open"
    );
}
