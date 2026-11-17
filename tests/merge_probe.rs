//! The Merge section draws its header, conflict list and aligned source panes
//! from an injected session; a real session comes from the core read model,
//! so this probe keeps the page honest about the shape it consumes.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use warp_station::net::UiEvent;
use warp_station::store::Store;
use warp_station::ui::merge::MergePage;
use warp_station_core::git::merge::{
    ConflictBlock, LineSpan, MergeFile, MergeOperation, MergeRow, MergeSession, TextMerge,
    WholeFileConflict, WholeFileReason,
};
use warp_station_core::repo_store::{RepoEntry, StatusProbe};

/// Points HOME at a scratch folder once, so no probe reads or writes the
/// developer's real `~/.warp`.
fn scratch_home() {
    static HOME: std::sync::Once = std::sync::Once::new();
    HOME.call_once(|| {
        let home =
            std::env::temp_dir().join(format!("warp-station-merge-probe-{}", std::process::id()));
        std::fs::create_dir_all(home.join(".warp")).expect("scratch home");
        // SAFETY: process-global env; every probe here calls this first.
        // USERPROFILE too: Windows resolves the data root from it first.
        unsafe {
            std::env::set_var("HOME", &home);
            #[cfg(windows)]
            std::env::set_var("USERPROFILE", &home);
        };
    });
}

/// Scratch settings for one probe, keyed like the other page probes so
/// parallel tests never share a file.
fn scratch_service(name: &str) -> std::sync::Arc<warp_station_core::settings::SettingsService> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "warp-station-probe-merge-{name}-{}-{seq}.json",
        std::process::id()
    ));
    std::fs::write(&path, "{}").expect("scratch settings");
    std::sync::Arc::new(warp_station_core::settings::SettingsService::at(path))
}

/// One conflicted file in the shape the core reader returns for a plain merge.
fn session() -> MergeSession {
    MergeSession {
        operation: MergeOperation::Merge,
        files: vec![MergeFile::Text(TextMerge {
            path: "a.txt".into(),
            ours: vec!["main".into()],
            theirs: vec!["feature".into()],
            result: vec![
                "<<<<<<< HEAD".into(),
                "main".into(),
                "=======".into(),
                "feature".into(),
                ">>>>>>> feature".into(),
            ],
            blocks: vec![ConflictBlock {
                result: LineSpan { start: 0, len: 5 },
                ours: Some(LineSpan { start: 0, len: 1 }),
                theirs: Some(LineSpan { start: 0, len: 1 }),
                ours_text: vec!["main".into()],
                theirs_text: vec!["feature".into()],
                base_text: Vec::new(),
            }],
            rows: vec![MergeRow {
                ours: Some(0),
                theirs: Some(0),
                base: None,
                block: Some(0),
            }],
            base: Vec::new(),
        })],
    }
}

/// The same conflict with the base section a diff3 marker style carries: the
/// ancestor column has something to show, so the page offers its toggle.
fn diff3_session() -> MergeSession {
    MergeSession {
        operation: MergeOperation::Merge,
        files: vec![MergeFile::Text(TextMerge {
            path: "a.txt".into(),
            ours: vec!["main".into()],
            theirs: vec!["feature".into()],
            result: vec![
                "<<<<<<< HEAD".into(),
                "main".into(),
                "||||||| merged common".into(),
                "base".into(),
                "=======".into(),
                "feature".into(),
                ">>>>>>> feature".into(),
            ],
            blocks: vec![ConflictBlock {
                result: LineSpan { start: 0, len: 7 },
                ours: Some(LineSpan { start: 0, len: 1 }),
                theirs: Some(LineSpan { start: 0, len: 1 }),
                ours_text: vec!["main".into()],
                theirs_text: vec!["feature".into()],
                base_text: vec!["base".into()],
            }],
            rows: vec![MergeRow {
                ours: Some(0),
                theirs: Some(0),
                base: Some(0),
                block: Some(0),
            }],
            base: vec!["base".into()],
        })],
    }
}

#[gpui_kit::gpui::test]
fn the_merge_page_draws_its_files_and_panes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("draw")));
    cx.update_entity(&store, |store: &mut Store, _| {
        let folder = "/tmp/merge-probe".to_string();
        store
            .merge_ops
            .insert(folder.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder.clone());
        store.merge = Some(session());
        store.merge_loaded_for = Some(folder);
        store.merge_generation = 1;
    });

    let (_handle, cx) = cx.add_window_view(|_window, cx| MergePage::new(store.clone(), cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    assert!(cx.debug_bounds("merge-page").is_some(), "the page draws");
    assert!(
        cx.debug_bounds("merge-header").is_some(),
        "the header draws"
    );
    assert!(
        cx.debug_bounds("merge-file-row").is_some(),
        "the conflict row draws"
    );
    assert!(
        cx.debug_bounds("merge-panes").is_some(),
        "the two source panes draw"
    );
    assert!(
        cx.debug_bounds("aligned-head").is_some(),
        "the kit's pane head draws the column labels"
    );
    assert!(
        cx.debug_bounds("aligned-panes").is_some(),
        "the diff kit's aligned body draws the rows"
    );
    assert!(
        cx.debug_bounds("merge-base").is_none(),
        "a default-style conflict has no ancestor to show"
    );
}

#[gpui_kit::gpui::test]
fn opening_the_editor_claims_the_detail(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("open")));
    cx.update_entity(&store, |store: &mut Store, cx| {
        let folder = "/probe/alpha".to_string();
        store
            .merge_ops
            .insert(folder.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder.clone());
        store.merge = Some(session());
        store.merge_loaded_for = Some(folder);
        store.merge_generation = 1;
        cx.notify();
    });
    assert!(
        cx.update_entity(&store, |store: &mut Store, cx| {
            store.open_merge_editor(cx);
            store.merge_editor_open
        }),
        "the detail switcher opens the merge editor"
    );
    assert!(
        cx.update_entity(&store, |store: &mut Store, _| {
            store.merge_file_for("a.txt").is_some()
        }),
        "Changes can tell a conflicted path from an untracked one"
    );
    cx.update_entity(&store, |store: &mut Store, cx| store.close_merge_editor(cx));
    assert!(
        !cx.update_entity(&store, |store: &mut Store, _| store.merge_editor_open),
        "Close leaves the editor"
    );
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

#[gpui_kit::gpui::test]
fn picking_lines_composes_the_result_and_saves(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let folder = std::env::temp_dir().join(format!(
        "warp-station-merge-probe-take-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("the fixture directory creates");
    let worktree = folder.join("a.txt");
    std::fs::write(
        &worktree,
        "<<<<<<< HEAD\nmain\n=======\nfeature\n>>>>>>> feature\n",
    )
    .expect("the fixture file writes");
    let folder_text = folder.to_string_lossy().to_string();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("take")));
    cx.update_entity(&store, |store: &mut Store, cx| {
        store
            .merge_ops
            .insert(folder_text.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder_text.clone());
        store.merge = Some(session());
        store.merge_loaded_for = Some(folder_text.clone());
        store.merge_generation = 1;
        cx.notify();
    });

    let (_window, cx) = cx.add_window_view(|_window, cx| MergePage::new(store.clone(), cx));
    draw(cx);
    // the Current line starts picked: adding Incoming composes both, in row
    // order, with Current first
    click(cx, "merge-pick-0-theirs-0");
    click(cx, "merge-save");
    let saved = std::fs::read_to_string(&worktree).expect("the result writes");
    assert_eq!(saved, "main\nfeature\n");
    // dropping the Current pick leaves only the Incoming line
    click(cx, "merge-pick-0-ours-0");
    click(cx, "merge-save");
    let saved = std::fs::read_to_string(&worktree).expect("the result writes");
    assert_eq!(saved, "feature\n");
    let _ = std::fs::remove_dir_all(&folder);
}

#[gpui_kit::gpui::test]
fn the_base_column_toggles_behind_its_button(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("base")));
    cx.update_entity(&store, |store: &mut Store, cx| {
        let folder = "/tmp/merge-probe-base".to_string();
        store
            .merge_ops
            .insert(folder.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder.clone());
        store.merge = Some(diff3_session());
        store.merge_loaded_for = Some(folder);
        store.merge_generation = 1;
        cx.notify();
    });

    let (_window, cx) = cx.add_window_view(|_window, cx| MergePage::new(store.clone(), cx));
    draw(cx);
    assert!(
        cx.debug_bounds("aligned-base").is_none(),
        "the ancestor column stays hidden until it is asked for"
    );
    assert!(
        cx.debug_bounds("merge-base").is_some(),
        "a file with base sections offers the toggle"
    );
    click(cx, "merge-base");
    assert!(
        cx.debug_bounds("aligned-base").is_some(),
        "the toggle draws the ancestor column beside the sides"
    );
}

#[gpui_kit::gpui::test]
fn a_whole_file_conflict_offers_both_sides(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let folder = "/tmp/merge-probe-whole".to_string();
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("whole")));
    cx.update_entity(&store, |store: &mut Store, cx| {
        store
            .merge_ops
            .insert(folder.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder.clone());
        store.merge = Some(MergeSession {
            operation: MergeOperation::Merge,
            files: vec![MergeFile::WholeFile(WholeFileConflict {
                path: "logo.png".into(),
                reason: WholeFileReason::Binary,
                ours_present: true,
                theirs_present: true,
            })],
        });
        store.merge_loaded_for = Some(folder);
        store.merge_generation = 1;
        cx.notify();
    });

    let (_window, cx) = cx.add_window_view(|_window, cx| MergePage::new(store.clone(), cx));
    draw(cx);
    assert!(
        cx.debug_bounds("merge-whole-ours").is_some(),
        "the ours take draws"
    );
    assert!(
        cx.debug_bounds("merge-whole-theirs").is_some(),
        "the theirs take draws"
    );
    assert!(
        cx.debug_bounds("merge-panes").is_none(),
        "a binary conflict has no text panes"
    );
}

#[gpui_kit::gpui::test]
fn a_clean_index_shows_the_resolved_state(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let folder = "/tmp/merge-probe-resolved".to_string();
    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("resolved")));
    cx.update_entity(&store, |store: &mut Store, cx| {
        store
            .merge_ops
            .insert(folder.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder.clone());
        store.merge = None;
        store.merge_loaded_for = Some(folder);
        store.merge_loading = false;
        store.merge_generation = 2;
        cx.notify();
    });

    let (_window, cx) = cx.add_window_view(|_window, cx| MergePage::new(store.clone(), cx));
    draw(cx);
    assert!(
        cx.debug_bounds("merge-resolved").is_some(),
        "a clean index says so"
    );
}

fn entry(name: &str, path: &str) -> RepoEntry {
    RepoEntry {
        name: name.into(),
        folder_path: path.into(),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: None,
        git_modified_count: 0,
        git_to_push_count: 0,
        git_to_pull_count: 0,
        git_last_commit_at: None,
        git_last_fetch_at: None,
        git_has_remote: false,
        git_branch_published: false,
        git_status_loaded: false,
        github_pr_count: 0,
        github_issue_count: 0,
        azure_pull_count: 0,
        github_web_url: None,
        azure_work_item_count: 0,
    }
}

/// A merge action re-probes one repository; that partial pass must not drop
/// the other repositories' stopped-operation flags.
#[gpui_kit::gpui::test]
fn a_single_repo_probe_keeps_the_other_flags(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("partial")));
    cx.update_entity(&store, |store: &mut Store, _| {
        store.replace_repos(vec![
            entry("alpha", "/probe/alpha"),
            entry("beta", "/probe/beta"),
        ]);
        store
            .merge_ops
            .insert("/probe/alpha".to_string(), MergeOperation::Merge);
        store
            .merge_ops
            .insert("/probe/beta".to_string(), MergeOperation::Rebase);
    });
    cx.update_entity(&store, |store: &mut Store, cx| {
        store.on_net_event(
            UiEvent::StatusSettled {
                probes: vec![StatusProbe {
                    folder_path: "/probe/alpha".into(),
                    merge: Some(MergeOperation::Merge),
                    ..Default::default()
                }],
            },
            cx,
        );
    });
    let ops = cx.update_entity(&store, |store: &mut Store, _| store.merge_ops.clone());
    assert_eq!(
        ops.get("/probe/beta"),
        Some(&MergeOperation::Rebase),
        "beta's flag survives alpha's partial probe"
    );
}

#[gpui_kit::gpui::test]
fn the_panes_own_the_conflict_keys(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let folder = std::env::temp_dir().join(format!(
        "warp-station-merge-probe-keys-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("the fixture directory creates");
    std::fs::write(
        folder.join("a.txt"),
        "<<<<<<< HEAD\nmain\n=======\nfeature\n>>>>>>> feature\n<<<<<<< HEAD\ntwo\n=======\ntheirs\n>>>>>>> feature\n",
    )
    .expect("the fixture file writes");
    let folder_text = folder.to_string_lossy().to_string();

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|_cx| Store::with_settings(net_tx, scratch_service("keys")));
    cx.update_entity(&store, |store: &mut Store, cx| {
        store
            .merge_ops
            .insert(folder_text.clone(), MergeOperation::Merge);
        store.selected_repo = Some(folder_text.clone());
        store.merge = Some(session());
        store.merge_loaded_for = Some(folder_text.clone());
        store.merge_generation = 1;
        cx.notify();
    });
    let (_window, cx) = cx.add_window_view(|_window, cx| MergePage::new(store.clone(), cx));
    draw(cx);

    // the panes own the Alt+Up/Down walk now: clicking them focuses the pane
    let panes = cx.debug_bounds("merge-panes").expect("the panes draw");
    cx.simulate_click(panes.center(), gpui::Modifiers::default());
    draw(cx);
    cx.simulate_keystrokes("alt-down");
    draw(cx);
    cx.simulate_keystrokes("alt-up");
    draw(cx);
    assert!(
        cx.debug_bounds("merge-page").is_some(),
        "the page survives the conflict keys"
    );
    let _ = std::fs::remove_dir_all(&folder);
}
