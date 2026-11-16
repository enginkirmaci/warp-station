//! The Merge section draws its header, conflict list and aligned source panes
//! from an injected session; a real session comes from the core read model,
//! so this probe keeps the page honest about the shape it consumes.

use gpui::TestAppContext;
use gpui::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use warp_station::net::UiEvent;
use warp_station::store::Store;
use warp_station::ui::merge::MergePage;
use warp_station::ui::window::MainWindow;
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
                block: Some(0),
            }],
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
        cx.debug_bounds("merge-pane-row").is_some(),
        "the source panes lay rows out instead of collapsing"
    );
}

#[gpui_kit::gpui::test]
fn the_rail_item_follows_the_stopped_operation(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    scratch_home();
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| Store::new(cx, net_tx));
    cx.update_entity(&store, |store: &mut Store, cx| {
        store
            .merge_ops
            .insert("/probe/alpha".to_string(), MergeOperation::Merge);
        cx.notify();
    });

    let (_window, cx) = cx.add_window_view(|window, cx| MainWindow::new(store.clone(), window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    cx.update(|window, cx| drop(window.draw(cx)));

    let rail = cx
        .debug_bounds("rail-merge")
        .expect("the rail item exists while an operation is stopped");
    cx.simulate_click(rail.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("merge-page").is_some(),
        "the Merge page opens from the rail item"
    );
    assert!(
        cx.debug_bounds("merge-loading").is_some(),
        "the page says it is reading the operation"
    );

    // the operation ends elsewhere: the next status pass drops the flag
    cx.update_entity(&store, |store: &mut Store, cx| {
        store.merge_ops.clear();
        cx.notify();
    });
    cx.run_until_parked();
    cx.update(|window, cx| drop(window.draw(cx)));
    assert!(
        cx.debug_bounds("rail-merge").is_none(),
        "the rail item leaves when nothing is stopped"
    );
    assert!(
        cx.debug_bounds("merge-page").is_none(),
        "and the section falls back to Repos"
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
fn taking_a_side_rewrites_the_buffer_and_saves(cx: &mut TestAppContext) {
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
    click(cx, "merge-take-ours-0");
    assert!(
        cx.debug_bounds("merge-take-ours-0").is_none(),
        "the block leaves the strip once it is taken"
    );
    click(cx, "merge-save");
    let saved = std::fs::read_to_string(&worktree).expect("the result writes");
    assert!(
        !saved.contains("<<<<<<<"),
        "the markers are gone: {saved:?}"
    );
    assert_eq!(saved, "main\n");
    let _ = std::fs::remove_dir_all(&folder);
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
fn alt_arrows_walk_the_conflict_strip(cx: &mut TestAppContext) {
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

    let strip = cx.debug_bounds("merge-conflicts").expect("the strip draws");
    cx.simulate_click(strip.center(), gpui::Modifiers::default());
    draw(cx);
    let first = cx
        .debug_bounds("merge-conflict-active")
        .expect("a conflict row is active")
        .origin
        .y;
    cx.simulate_keystrokes("alt-down");
    draw(cx);
    let second = cx
        .debug_bounds("merge-conflict-active")
        .expect("the next conflict becomes active")
        .origin
        .y;
    assert!(second > first, "Alt+Down moves the active conflict down");
    cx.simulate_keystrokes("alt-up");
    draw(cx);
    let back = cx
        .debug_bounds("merge-conflict-active")
        .expect("the first conflict is active again")
        .origin
        .y;
    assert_eq!(back, first, "Alt+Up returns to the first conflict");
    let _ = std::fs::remove_dir_all(&folder);
}
