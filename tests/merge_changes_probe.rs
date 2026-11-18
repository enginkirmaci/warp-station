//! The Changes tab's Merge Changes rail: a stopped operation lists its
//! unmerged paths above the ordinary Files list. It has its own head, cursor
//! and edge, and a step selects the path so the diff shows that conflict.

use gpui::prelude::*;
use gpui::{Context, Entity, Modifiers, Render, TestAppContext, VisualTestContext, Window, div};
use std::path::PathBuf;
use std::sync::Arc;
use warp_station::store::Store;
use warp_station::ui::changes::ChangesView;
use warp_station_core::git::merge::{
    ConflictBlock, LineSpan, MergeFile, MergeOperation, MergeSession, TextMerge, WholeFileConflict,
    WholeFileReason,
};
use warp_station_core::model::{GitChangeSnapshot, GitChangedFile};
use warp_station_core::repo_store::RepoEntry;
use warp_station_core::settings::SettingsService;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "warp-station-merge-changes-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    std::fs::write(&path, "{}").unwrap();
    path
}

fn repo() -> RepoEntry {
    RepoEntry {
        name: "alpha".into(),
        folder_path: PathBuf::from("/probe/alpha"),
        solution_path: None,
        tags: Vec::new(),
        git_branch_name: Some("main".into()),
        git_modified_count: 2,
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

fn changes(paths: &[&str]) -> GitChangeSnapshot {
    let unstaged: Vec<GitChangedFile> = paths
        .iter()
        .map(|path| GitChangedFile {
            path: (*path).to_string(),
            status_code: "U".into(),
            additions: Some(1),
            deletions: Some(0),
        })
        .collect();
    GitChangeSnapshot {
        status_files: unstaged.clone(),
        staged_files: Vec::new(),
        unstaged_files: unstaged,
        worktree_counts: Default::default(),
        cached_counts: Default::default(),
    }
}

fn block(index: usize) -> ConflictBlock {
    ConflictBlock {
        result: LineSpan {
            start: index as u32,
            len: 1,
        },
        ours: None,
        theirs: None,
        ours_text: Vec::new(),
        theirs_text: Vec::new(),
        base_text: Vec::new(),
    }
}

fn text_conflict(path: &str, blocks: usize) -> MergeFile {
    MergeFile::Text(TextMerge {
        path: path.into(),
        ours: Vec::new(),
        theirs: Vec::new(),
        result: Vec::new(),
        blocks: (0..blocks).map(block).collect(),
        rows: Vec::new(),
        base: Vec::new(),
    })
}

fn session(files: Vec<MergeFile>) -> MergeSession {
    MergeSession {
        operation: MergeOperation::Merge,
        files,
    }
}

struct Host {
    changes: Entity<ChangesView>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(self.changes.clone().into_any_element())
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

/// The Changes view for repo alpha with a stopped operation on `files` and the
/// worktree showing `changed`.
fn open_changes<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    changed: &[&str],
    files: Option<Vec<MergeFile>>,
) -> &'a mut VisualTestContext {
    cx.update(gpui_kit::init);
    cx.update(warp_station::theme::apply_kit_theme);

    let (net_tx, _net_rx) = async_channel::unbounded();
    let store = cx.new(|cx| {
        let mut store = Store::with_settings(net_tx, Arc::new(SettingsService::at(scratch(name))));
        store.replace_repos(vec![repo()]);
        store.select_repo("/probe/alpha", cx);
        store.changes_loaded_for = Some("/probe/alpha".into());
        store.set_changes(Some(changes(changed)));
        store
    });
    if let Some(files) = files {
        store.update(cx, |store, _| {
            store
                .merge_ops
                .insert("/probe/alpha".into(), MergeOperation::Merge);
            store.merge_loaded_for = Some("/probe/alpha".into());
            store.merge = Some(session(files));
        });
    }

    let (_host, vcx) = cx.add_window_view(|window, cx| {
        let changes = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        Host { changes }
    });
    draw(vcx);
    draw(vcx);
    vcx
}

#[gpui::test]
fn a_stopped_operation_lists_its_files_above_the_files_list(cx: &mut TestAppContext) {
    let vcx = open_changes(
        cx,
        "list",
        &["a.txt", "b.txt"],
        Some(vec![text_conflict("a.txt", 2), text_conflict("b.txt", 1)]),
    );

    assert!(
        vcx.debug_bounds("changes-merge").is_some(),
        "the Merge Changes rail draws over the Files list"
    );
    assert!(
        vcx.debug_bounds("changes-merge-0").is_some()
            && vcx.debug_bounds("changes-merge-1").is_some(),
        "one row per unmerged path"
    );
    assert!(
        vcx.debug_bounds("files-head-idle").is_some()
            || vcx.debug_bounds("files-head-focused").is_some(),
        "the ordinary Files list keeps its head below it"
    );
    assert!(
        vcx.debug_bounds("merge-head-idle").is_some(),
        "the rail starts unfocused like every other list"
    );
    let merge = vcx.debug_bounds("changes-merge").unwrap();
    let files = vcx.debug_bounds("files-head-idle").unwrap();
    assert!(
        merge.bottom() <= files.top(),
        "Merge Changes sits above the Files list: merge {merge:?}, files {files:?}"
    );
}

#[gpui::test]
fn the_merge_cursor_walks_the_conflicts_and_selects_them(cx: &mut TestAppContext) {
    let vcx = open_changes(
        cx,
        "keys",
        &["a.txt", "b.txt"],
        Some(vec![text_conflict("a.txt", 2), text_conflict("b.txt", 1)]),
    );
    // the first file is the selection, so its conflict card is the diff
    assert!(
        vcx.debug_bounds("changes-merge-state-a.txt").is_some(),
        "the selected conflict draws its merge state"
    );

    click(vcx, "changes-merge-0");
    assert!(
        vcx.debug_bounds("merge-head-focused").is_some(),
        "a press focuses the rail"
    );

    vcx.simulate_keystrokes("down");
    draw(vcx);
    assert!(
        vcx.debug_bounds("changes-merge-state-b.txt").is_some(),
        "↓ selects the next conflict"
    );

    vcx.simulate_keystrokes("up");
    draw(vcx);
    assert!(
        vcx.debug_bounds("changes-merge-state-a.txt").is_some(),
        "↑ selects the previous conflict"
    );

    vcx.simulate_keystrokes("end");
    draw(vcx);
    assert!(
        vcx.debug_bounds("changes-merge-state-b.txt").is_some(),
        "End goes to the last conflict"
    );

    vcx.simulate_keystrokes("home");
    draw(vcx);
    assert!(
        vcx.debug_bounds("changes-merge-state-a.txt").is_some(),
        "Home goes to the first conflict"
    );

    // ← from the Files list lands on the rail: the two are `←`/`→` stops, top
    // to bottom, and the file list is not the only way in
    click(vcx, "files-head-idle");
    assert!(
        vcx.debug_bounds("files-head-focused").is_some(),
        "a press on the Files head focuses that list"
    );
    vcx.simulate_keystrokes("left");
    draw(vcx);
    assert!(
        vcx.debug_bounds("merge-head-focused").is_some(),
        "← from the Files list focuses Merge Changes"
    );
    vcx.simulate_keystrokes("down");
    draw(vcx);
    assert!(
        vcx.debug_bounds("changes-merge-state-b.txt").is_some(),
        "the rail's cursor moves after taking the keyboard"
    );
}

#[gpui::test]
fn a_long_conflict_list_scrolls_inside_its_cap(cx: &mut TestAppContext) {
    let files: Vec<MergeFile> = (0..10)
        .map(|index| text_conflict(&format!("conflict_{index:02}.txt"), 1))
        .collect();
    let paths: Vec<String> = (0..10)
        .map(|index| format!("conflict_{index:02}.txt"))
        .collect();
    let changed: Vec<&str> = paths.iter().map(String::as_str).collect();
    let vcx = open_changes(cx, "cap", &changed, Some(files));

    let list = vcx
        .debug_bounds("changes-merge-list")
        .expect("the rail's scroller draws");
    assert!(
        f32::from(list.size.height) <= 214.,
        "the rail stops at its cap instead of eating the Files list: {list:?}"
    );
}

/// A whole-file conflict still gets a row: its reason replaces the block
/// count, and it is the same list.
#[gpui::test]
fn a_whole_file_conflict_keeps_its_row(cx: &mut TestAppContext) {
    let vcx = open_changes(
        cx,
        "whole",
        &["image.bin"],
        Some(vec![MergeFile::WholeFile(WholeFileConflict {
            path: "image.bin".into(),
            reason: WholeFileReason::Binary,
            ours_present: true,
            theirs_present: true,
        })]),
    );
    assert!(vcx.debug_bounds("changes-merge-0").is_some());
    assert!(
        vcx.debug_bounds("changes-merge-state-image.bin").is_some(),
        "the selected whole-file conflict draws its state"
    );
    let row = vcx.debug_bounds("changes-merge-0").unwrap();
    assert!(
        f32::from(row.size.height) >= 30.,
        "the row is a list row, not a bare line: {row:?}"
    );
}

/// Nothing is stopped: no Merge Changes rail, and the Files list has the tab
/// to itself.
#[gpui::test]
fn a_quiet_repository_has_no_merge_rail(cx: &mut TestAppContext) {
    let vcx = open_changes(cx, "quiet", &["a.txt"], None);
    assert!(
        vcx.debug_bounds("changes-merge").is_none(),
        "there is no rail without a stopped operation"
    );
    assert!(vcx.debug_bounds("files-head-idle").is_some());
}
