//! The split main screen's detail rail must dispatch, not just draw: the star
//! toggles the repo's favourite flag and the sync verbs reach the net thread.
//! The table probe (repos_probe) pins `DEVTOOLS_UI_TABLE=1`; this one stays on
//! the default split layout, which is what the app ships.

use devtools::net::{GitAction, NetCommand};
use devtools::store::Store;
use devtools::ui::window::MainWindow;
use devtools_core::repo_store::RepoEntry;
use devtools_core::settings::RepoSortMode;
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

/// A store on scratch settings: what a probe saves must not reach the
/// developer's file, and what it reads must not depend on it.
fn scratch_store(
    cx: &mut TestAppContext,
    name: &str,
    repos: Vec<RepoEntry>,
) -> (gpui::Entity<Store>, async_channel::Receiver<NetCommand>) {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "devtools-probe-split-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(&scratch, "{}").expect("scratch settings");
    let (net_tx, net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(devtools_core::settings::SettingsService::at(scratch)),
        );
        store.group_by_root = false;
        store.replace_repos(repos);
        store
    });
    (store, net_rx)
}

/// Two folder groups, for the probes that drive the bands.
fn grouped_store(cx: &mut TestAppContext, name: &str) -> gpui::Entity<Store> {
    cx.update(gpui_kit::init);
    cx.update(devtools::theme::apply_kit_theme);

    let scratch = std::env::temp_dir().join(format!(
        "devtools-probe-split-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(
        &scratch,
        r#"{"Repos": {"RepoScanFolders": ["/roots/a", "/second/b"], "GroupReposByRoot": true}}"#,
    )
    .expect("scratch settings");
    let (net_tx, _net_rx) = async_channel::unbounded();
    cx.new(|_cx| {
        let mut store = Store::with_settings(
            net_tx,
            std::sync::Arc::new(devtools_core::settings::SettingsService::at(scratch)),
        );
        store.replace_repos(vec![
            entry("alpha", "/roots/a/alpha"),
            entry("beta", "/roots/a/beta"),
            entry("gamma", "/second/b/gamma"),
        ]);
        store
    })
}

/// The projected repo rows, top to bottom.
fn row_names(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.read_entity(store, |s, _| {
        s.projected
            .iter()
            .filter_map(|row| row.repo_index())
            .map(|ix| s.repos[ix].name.clone())
            .collect()
    })
}

fn open_window<'a>(
    store: &gpui::Entity<Store>,
    cx: &'a mut TestAppContext,
) -> &'a mut gpui::VisualTestContext {
    let handle = store.clone();
    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(handle, window, cx));
    draw(cx);
    cx
}

fn clipboard_text(cx: &gpui::VisualTestContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

fn scan_folders(store: &gpui::Entity<Store>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.read_entity(store, |s, _| s.settings().repos.repo_scan_folders)
        .unwrap_or_default()
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

/// The rail's keycaps are a promise: a letter runs its row for the repo the
/// rail shows. warp-agent's row is the one a probe can press: it only messages
/// the net thread, where the others start real processes.
#[gpui::test]
fn a_letter_key_runs_its_launcher(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "letters", vec![entry("alpha", "/probe/alpha")]);
    let exe = std::env::current_exe().expect("test binary path");
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| settings.warp_agent.executable = exe.display().to_string());
    });
    let cx = open_window(&store, cx);

    cx.simulate_keystrokes("a");
    cx.run_until_parked();

    let mut opened = None;
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::WarpOpen { folder, .. } = command {
            opened = Some(folder);
        }
    }
    assert_eq!(
        opened,
        Some(PathBuf::from("/probe/alpha")),
        "A runs the warp-agent row"
    );
}

/// A held letter repeats; a launcher row must not start once per repeat.
#[gpui::test]
fn a_held_letter_does_not_repeat(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "held", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    cx.simulate_event(gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("s").expect("a keystroke"),
        is_held: true,
        prefer_character_input: false,
    });
    draw(cx);

    assert!(
        !cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
        "the repeat of a held S is ignored"
    );
}

/// The row keys act on the list, so nothing open on top of it may let them
/// through: they used to star repos and move the selection behind the sheet.
#[gpui::test]
fn the_keymap_stays_out_of_open_layers(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "layers",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    let assert_rows_untouched = |cx: &mut gpui::VisualTestContext, layer: &str| {
        cx.simulate_keystrokes("s");
        draw(cx);
        cx.simulate_keystrokes("down");
        draw(cx);
        assert_eq!(
            selected(&store, cx).as_deref(),
            Some("/probe/alpha"),
            "↓ stays out of the {layer}"
        );
        assert!(
            !cx.read_entity(&store, |s, _| s.repos[0].is_favorite()),
            "S stays out of the {layer}"
        );
    };

    cx.simulate_keystrokes("c");
    draw(cx);
    draw(cx);
    assert_rows_untouched(cx, "Changes sheet");
    cx.simulate_keystrokes("escape");
    draw(cx);

    click(cx, "insp-more");
    assert_rows_untouched(cx, "repo menu");
    cx.simulate_keystrokes("escape");
    draw(cx);

    click(cx, "sort-button");
    assert_rows_untouched(cx, "sort menu");
    cx.simulate_keystrokes("escape");
    draw(cx);

    click(cx, "add-repo-button");
    assert_rows_untouched(cx, "Add repo dialog");
}

#[gpui::test]
fn pressing_a_band_folds_its_group_and_persists(cx: &mut TestAppContext) {
    let store = grouped_store(cx, "fold");
    let cx = open_window(&store, cx);
    let collapsed = |cx: &mut gpui::VisualTestContext| {
        cx.read_entity(&store, |s, _| s.settings().repos.collapsed_groups)
    };
    assert_eq!(row_names(&store, cx), ["alpha", "beta", "gamma"]);

    click(cx, "repo-band-/roots/a");
    assert_eq!(row_names(&store, cx), ["gamma"]);
    assert_eq!(collapsed(cx), Some(vec!["/roots/a".to_string()]));

    click(cx, "repo-band-/roots/a");
    assert_eq!(row_names(&store, cx), ["alpha", "beta", "gamma"]);
    assert_eq!(collapsed(cx), None);
}

/// The folder glyph sits inside the band, so its press must not fold too.
#[gpui::test]
fn the_folder_glyph_solos_its_group_without_folding_it(cx: &mut TestAppContext) {
    let store = grouped_store(cx, "solo");
    let cx = open_window(&store, cx);
    let soloed =
        |cx: &mut gpui::VisualTestContext| cx.read_entity(&store, |s, _| s.group_filter.clone());

    click(cx, "repo-band-solo-/roots/a");
    assert_eq!(soloed(cx).as_deref(), Some("/roots/a"));
    assert_eq!(
        row_names(&store, cx),
        ["alpha", "beta"],
        "the group keeps its rows and the others leave"
    );

    click(cx, "repo-band-solo-/roots/a");
    assert_eq!(soloed(cx), None, "the same glyph lifts the filter");

    click(cx, "repo-band-solo-/roots/a");
    click(cx, "clear-button");
    assert_eq!(soloed(cx), None, "and so does Clear");
    assert_eq!(row_names(&store, cx), ["alpha", "beta", "gamma"]);
}

#[gpui::test]
fn the_sort_menu_reorders_the_list_and_persists(cx: &mut TestAppContext) {
    let mut old = entry("alpha", "/probe/alpha");
    old.git_last_commit_at = Some(chrono::Local::now() - chrono::Duration::days(2));
    let mut new = entry("beta", "/probe/beta");
    new.git_last_commit_at = Some(chrono::Local::now());
    let (store, _net_rx) = scratch_store(cx, "sort", vec![old, new]);
    let cx = open_window(&store, cx);
    assert_eq!(row_names(&store, cx), ["alpha", "beta"]);

    click(cx, "sort-button");
    click(cx, "sort-mode-Last activity");
    assert_eq!(
        row_names(&store, cx),
        ["beta", "alpha"],
        "a new mode starts in its own default direction: newest first"
    );
    assert_eq!(
        cx.read_entity(&store, |s, _| s.settings().repos.sort_mode),
        RepoSortMode::LastActivity,
        "the choice is saved"
    );
    assert!(
        cx.debug_bounds("sort-menu").is_none(),
        "picking closes the menu"
    );

    click(cx, "sort-button");
    click(cx, "sort-mode-Last activity");
    assert_eq!(
        row_names(&store, cx),
        ["alpha", "beta"],
        "picking the active mode flips it"
    );
}

/// With GitHub off nothing ranks repos by its counts, so the menu drops it.
#[gpui::test]
fn the_sort_menu_follows_the_github_switch(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "sort-github", vec![entry("alpha", "/probe/alpha")]);
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| settings.repos.enable_git_hub = false);
    });
    let cx = open_window(&store, cx);

    click(cx, "sort-button");

    assert!(cx.debug_bounds("sort-mode-GitHub").is_none());
    assert!(cx.debug_bounds("sort-mode-Attention first").is_some());
}

#[gpui::test]
fn the_sort_menu_closes_on_outside_presses_and_escape(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "sort-close",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    click(cx, "sort-button");
    assert!(cx.debug_bounds("sort-menu").is_some(), "it opens");
    click(cx, "repo-row-1");
    assert!(
        cx.debug_bounds("sort-menu").is_none(),
        "a press elsewhere closes it"
    );
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "and still lands on what it hit"
    );

    click(cx, "sort-button");
    click(cx, "sort-button");
    assert!(
        cx.debug_bounds("sort-menu").is_none(),
        "the button toggles rather than closing and reopening"
    );

    click(cx, "sort-button");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("sort-menu").is_none());
}

/// The star sits on the row, but pressing it is not a press on the row.
#[gpui::test]
fn the_row_star_toggles_the_favorite_without_selecting(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "row-star",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    click(cx, "repo-star-1");

    assert!(
        cx.read_entity(&store, |s, _| s.repos[1].is_favorite()),
        "beta's star marks beta"
    );
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/alpha"),
        "the selection stays where it was"
    );
}

#[gpui::test]
fn an_archived_row_has_no_star_to_press(cx: &mut TestAppContext) {
    let mut archived = entry("beta", "/probe/beta");
    archived.tags = vec![devtools_core::repo_store::ARCHIVED_TAG.to_string()];
    let (store, _net_rx) = scratch_store(
        cx,
        "row-archived",
        vec![entry("alpha", "/probe/alpha"), archived],
    );
    cx.update_entity(&store, |s: &mut Store, cx| s.toggle_show_archived(cx));
    let cx = open_window(&store, cx);

    click(cx, "repo-star-1");

    assert!(!cx.read_entity(&store, |s, _| s.repos[1].is_favorite()));
}

/// "clean" means nothing at all is waiting: a repo with only a PR open or
/// commits to pull used to read as clean, because the row never showed them.
#[gpui::test]
fn a_row_shows_all_that_waits_on_it(cx: &mut TestAppContext) {
    let quiet = |name: &str, path: &str| {
        let mut repo = entry(name, path);
        repo.git_modified_count = 0;
        repo.git_to_push_count = 0;
        repo
    };
    let mut pulled = quiet("alpha", "/probe/alpha");
    pulled.git_to_pull_count = 2;
    let mut reviewed = quiet("beta", "/probe/beta");
    reviewed.github_pr_count = 1;
    let (store, _net_rx) = scratch_store(
        cx,
        "row-meta",
        vec![pulled, reviewed, quiet("gamma", "/probe/gamma")],
    );
    let cx = open_window(&store, cx);

    assert!(cx.debug_bounds("repo-behind-0").is_some());
    assert!(cx.debug_bounds("repo-clean-0").is_none());
    assert!(cx.debug_bounds("repo-prs-1").is_some());
    assert!(cx.debug_bounds("repo-clean-1").is_none());
    assert!(cx.debug_bounds("repo-clean-2").is_some());
}

#[gpui::test]
fn the_github_switch_takes_its_counts_off_the_row(cx: &mut TestAppContext) {
    let mut repo = entry("alpha", "/probe/alpha");
    repo.git_modified_count = 0;
    repo.git_to_push_count = 0;
    repo.github_pr_count = 1;
    let (store, _net_rx) = scratch_store(cx, "row-github", vec![repo]);
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| settings.repos.enable_git_hub = false);
    });
    let cx = open_window(&store, cx);

    assert!(cx.debug_bounds("repo-prs-0").is_none());
    assert!(cx.debug_bounds("repo-clean-0").is_some());
}

#[gpui::test]
fn a_row_shows_its_azure_state_while_azure_is_on(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "row-azure", vec![entry("alpha", "/probe/alpha")]);
    cx.update_entity(&store, |s: &mut Store, _| {
        let snapshot = devtools_core::azure::AzureSnapshot {
            available: true,
            pulls: vec![devtools_core::azure::AzureItem::default()],
            ..Default::default()
        };
        s.azure_by_folder
            .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
    });
    let cx = open_window(&store, cx);
    assert!(cx.debug_bounds("repo-azure-0").is_some());

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.update_settings(|settings| settings.repos.enable_azure_dev_ops = false);
        cx.notify();
    });
    draw(cx);
    assert!(cx.debug_bounds("repo-azure-0").is_none());
}

#[gpui::test]
fn a_row_marks_a_running_warp_agent_workspace(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "row-warp", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    assert!(cx.debug_bounds("repo-warp-0").is_none());

    cx.update_entity(&store, |s: &mut Store, cx| {
        s.warp_running.insert(
            "/probe/alpha".to_string(),
            devtools::net::WarpWorkspaceStatus {
                connected: true,
                running: vec!["chat".into()],
            },
        );
        cx.notify();
    });
    draw(cx);

    assert!(cx.debug_bounds("repo-warp-0").is_some());
}

#[gpui::test]
fn the_repo_menu_copies_the_folder_path(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "copy-path", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "insp-menu-copy-path");

    assert_eq!(clipboard_text(cx).as_deref(), Some("/probe/alpha"));
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the row closes the menu behind its action"
    );
}

#[gpui::test]
fn the_repo_menu_refreshes_the_repo(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(cx, "refresh", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    load_changes_folders(&net_rx);

    click(cx, "insp-more");
    click(cx, "insp-menu-refresh");

    assert_eq!(
        load_changes_folders(&net_rx),
        vec!["/probe/alpha".to_string()],
        "Refresh reloads the rail's changes"
    );
}

/// Nothing links to GitHub before a pass has named the repo's page, and either
/// pass can be the first to answer.
#[gpui::test]
fn the_github_rows_wait_for_a_url(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "github-url", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    assert!(
        cx.debug_bounds("insp-menu-github").is_none(),
        "no URL yet, no GitHub row"
    );

    cx.update_entity(&store, |s: &mut Store, cx| {
        let snapshot = devtools_core::github::GitHubSnapshot {
            url: Some("https://example.invalid/alpha".into()),
            available: true,
            ..Default::default()
        };
        s.github_by_folder
            .insert("/probe/alpha".to_string(), std::sync::Arc::new(snapshot));
        cx.notify();
    });
    draw(cx);
    assert!(
        cx.debug_bounds("insp-menu-github").is_some(),
        "the activity pass names the page"
    );

    click(cx, "insp-menu-copy-url");
    assert_eq!(
        clipboard_text(cx).as_deref(),
        Some("https://example.invalid/alpha")
    );
}

#[gpui::test]
fn removing_a_repo_takes_two_clicks(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(
        cx,
        "remove",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| {
            settings.repos.repo_scan_folders =
                Some(vec!["/probe/alpha".into(), "/probe/beta".into()]);
        });
    });
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "insp-menu-remove");
    assert_eq!(
        scan_folders(&store, cx).len(),
        2,
        "the first click only arms the row"
    );
    assert!(
        cx.debug_bounds("insp-repo-menu").is_some(),
        "and keeps the menu open for the second"
    );

    click(cx, "insp-menu-remove");
    assert_eq!(scan_folders(&store, cx), ["/probe/beta"]);
    assert!(cx.debug_bounds("insp-repo-menu").is_none());

    let mut rescanned = false;
    while let Ok(command) = net_rx.try_recv() {
        rescanned |= matches!(command, NetCommand::Scan { .. });
    }
    assert!(rescanned, "the list rebuilds without the repo");
}

/// An armed Remove must never fire on a repo it wasn't armed for.
#[gpui::test]
fn a_moved_selection_closes_the_menu_and_disarms_it(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "disarm",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| {
            settings.repos.repo_scan_folders =
                Some(vec!["/probe/alpha".into(), "/probe/beta".into()]);
        });
    });
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "insp-menu-remove");

    // a search hit, say: anything that moves the selection without a press
    cx.update_entity(&store, |s: &mut Store, cx| {
        s.ensure_repo_selected("/probe/beta", cx);
    });
    draw(cx);
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the menu belongs to the repo it opened on"
    );

    click(cx, "insp-more");
    click(cx, "insp-menu-remove");
    assert_eq!(
        scan_folders(&store, cx).len(),
        2,
        "the reopened menu starts disarmed"
    );
}

#[gpui::test]
fn the_repo_menu_closes_on_outside_presses_and_escape(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(
        cx,
        "menu-close",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let cx = open_window(&store, cx);

    click(cx, "insp-more");
    click(cx, "repo-row-1");
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "a press elsewhere closes it"
    );
    assert_eq!(
        selected(&store, cx).as_deref(),
        Some("/probe/beta"),
        "and still lands on what it hit"
    );

    click(cx, "insp-more");
    click(cx, "insp-more");
    assert!(
        cx.debug_bounds("insp-repo-menu").is_none(),
        "the ⋯ button toggles rather than closing and reopening"
    );

    click(cx, "insp-more");
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(cx.debug_bounds("insp-repo-menu").is_none());
}

#[gpui::test]
fn a_tag_chip_removes_its_tag(cx: &mut TestAppContext) {
    let mut tagged = entry("alpha", "/probe/alpha");
    tagged.tags = vec!["rust".into(), "llm".into()];
    let (store, _net_rx) = scratch_store(cx, "tags", vec![tagged]);
    let cx = open_window(&store, cx);

    click(cx, "insp-tag-remove-rust");

    assert_eq!(
        cx.read_entity(&store, |s, _| s.repos[0].tags.clone()),
        ["llm"]
    );
}

#[gpui::test]
fn the_branch_line_copies_the_branch(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "branch", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);

    click(cx, "insp-branch");

    assert_eq!(clipboard_text(cx).as_deref(), Some("main"));
}

/// The rail only shows what GitHub answered: nothing before the answer, and
/// nothing for a repo it knows no facts about.
#[gpui::test]
fn the_details_section_follows_the_github_answer(cx: &mut TestAppContext) {
    let (store, _net_rx) = scratch_store(cx, "details", vec![entry("alpha", "/probe/alpha")]);
    let cx = open_window(&store, cx);
    assert!(cx.debug_bounds("insp-details").is_none());

    let answer = |cx: &mut gpui::VisualTestContext, details| {
        cx.update_entity(&store, |s: &mut Store, cx| {
            s.github_details = Some(details);
            cx.notify();
        });
        draw(cx);
    };

    answer(
        cx,
        devtools_core::github::GitHubRepoDetails {
            owner: Some("enginkirmaci".into()),
            primary_language: Some("Rust".into()),
            topics: vec!["gpui".into()],
            ..Default::default()
        },
    );
    assert!(cx.debug_bounds("insp-details").is_some());

    answer(cx, Default::default());
    assert!(cx.debug_bounds("insp-details").is_none());
}

/// The details used to be requested by the Overview tab's view, which only
/// lived on because the hidden workspace bar held it.
#[gpui::test]
fn selecting_a_row_asks_github_for_its_details(cx: &mut TestAppContext) {
    let (store, net_rx) = scratch_store(
        cx,
        "details-ask",
        vec![entry("alpha", "/probe/alpha"), entry("beta", "/probe/beta")],
    );
    let exe = std::env::current_exe().expect("test binary path");
    cx.read_entity(&store, |s, _| {
        s.update_settings(|settings| {
            settings.repos.git_hub_executable = exe.display().to_string();
        });
    });
    let cx = open_window(&store, cx);

    click(cx, "repo-row-1");

    let mut asked = Vec::new();
    while let Ok(command) = net_rx.try_recv() {
        if let NetCommand::GitHubDetails { folder, .. } = command {
            asked.push(folder);
        }
    }
    assert_eq!(
        asked,
        [PathBuf::from("/probe/alpha"), PathBuf::from("/probe/beta")],
        "the row the rail opens on, then the pressed one"
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
