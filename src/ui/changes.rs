//! The Changes and History tabs: the selected repo's commit workspace. Changes
//! is the git toolbar (branch, Fetch, Pull, Push, Sync) over the file list and
//! commit box on the left and the selected file's diff on the right. History is
//! the day-grouped commits on the left and the selected commit's head band over
//! a file rail and that file's diff on the right. The heavy data lives on the
//! store; this view owns only ephemeral state (the selected file, fetched
//! patches, dropdown state, the commit draft).

use crate::net::{GitAction, NetCommand};
use crate::store::{PullShortcut, Store};
use crate::theme;
use crate::ui::diff_controls;
use crate::ui::file_tree;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{
    Context, Entity, FocusHandle, FontWeight, ListAlignment, ListState, MouseButton,
    ScrollStrategy, SharedString, UniformListScrollHandle, Window, div, list, px, uniform_list,
};
use gpui_kit::component::input::{InputState, TextareaState};
use std::cell::RefCell;
use std::rc::Rc;
use warp_station_core::model::GitChangedFile;
use warp_station_core::settings::DiffViewMode;
use warpui_kit_core::{list_nav, motion, pane_nav};
use warpui_kit_diff::{DiffScrollHandles, render_diff_with_scroll};
use warpui_kit_picker::{
    FilteredPicker, OverlayAnchor, PanelChrome, PickerBody, PickerData, PickerIcons, PickerItem,
    PickerRow, PickerSpec, PickerTarget, Selection, TriggerScale,
};

/// One history row's display strings, precomputed when the history (or the
/// day) changes so a render never re-derives them per row.
#[derive(Clone)]
struct HistoryRow {
    /// Some(..) on the first commit of a day: the group header label.
    day_label: Option<String>,
    initials: String,
    author: String,
    rel: String,
    short_hash: String,
    subject: String,
    hash: String,
}

/// A History list item: a day's header row or a commit row. The list is
/// height-measured (`gpui::list`), so a 26 px day header never squeezes the
/// commit under it.
#[derive(Clone)]
enum HistoryItem {
    Day(String),
    Commit(usize),
}

/// The staged/unstaged split produced by `GitChangeSnapshot::change_groups`.
type GroupedFiles = (Vec<GitChangedFile>, Vec<GitChangedFile>);

/// One snapshot's grouped split plus the flat unstaged path list the header's
/// discard-all hands to git. Both are built once per generation: a 10k-file
/// tree must not recollect its paths on every repaint.
struct FileGroups {
    groups: GroupedFiles,
    unstaged_paths: Rc<Vec<String>>,
    /// The section trees, built with the groups so a render never walks paths.
    staged_tree: file_tree::Tree,
    unstaged_tree: file_tree::Tree,
}

/// Which of the file list's two sections a header folds.
#[derive(Clone, Copy)]
enum FileSection {
    Staged,
    Unstaged,
}

/// One fixed-height item of the virtualized file list: a section header, a
/// folder, or a row on one of the two sides.
#[derive(Clone)]
enum ChangeListItem {
    StagedHeader,
    StagedFolder(FolderItem),
    StagedFile { index: usize, depth: usize },
    UnstagedHeader,
    UnstagedFolder(FolderItem),
    UnstagedFile { index: usize, depth: usize },
}

/// A folder row's display data, precomputed with the tree so a render clones
/// two refcounted strings and two numbers per visible folder.
#[derive(Clone)]
struct FolderItem {
    /// The deepest folder of the row's chain: the fold key.
    path: Rc<str>,
    /// The compacted label (`src/ui`).
    label: Rc<str>,
    /// Changed files at or below the folder.
    count: usize,
    depth: usize,
}

/// Which row the list's cursor is on. The cursor is not the selection: a
/// folder row can own it while the diff keeps the last file.
#[derive(Clone, Eq, PartialEq)]
struct RowKey {
    path: String,
    staged: bool,
    folder: bool,
}

/// One row the cursor can land on, with its index in the built item list.
struct CursorRow {
    item_ix: usize,
    key: RowKey,
    depth: usize,
}

/// One row of the History commit rail: the tree's folders and files, or the
/// flat list when the app-wide switch is off.
#[derive(Clone)]
enum RailItem {
    Folder(FolderItem),
    File { index: usize, depth: usize },
}

/// The History rail's cursor. It is not the selection: a folder row can own it
/// while the diff keeps the selected file.
#[derive(Clone, Eq, PartialEq)]
struct RailKey {
    path: String,
    folder: bool,
}

/// One cursor stop of the rail, with the item it points at.
struct RailRow {
    item_ix: usize,
    key: RailKey,
    depth: usize,
    /// The file index, when the row is a file.
    index: Option<usize>,
}

impl FileSection {
    fn label(self) -> &'static str {
        match self {
            Self::Staged => "Staged Changes",
            Self::Unstaged => "Unstaged Changes",
        }
    }

    fn selector(self) -> &'static str {
        match self {
            Self::Staged => "changes-section-staged",
            Self::Unstaged => "changes-section-unstaged",
        }
    }
}

/// One history row: a commit, with its day header on the row above the first
/// of each day. Fixed so `uniform_list` can size the whole list.
const HISTORY_ROW_HEIGHT: f32 = 58.;
/// A day header's own row; the commit list is height-measured, so this stays
/// out of the commit row's 58 px.
const HISTORY_DAY_HEIGHT: f32 = 26.;
/// The file list and commit box's width.
const FILE_COLUMN_W: f32 = 320.;
/// The trailing zone a row keeps for its counts and, on hover, its verbs: the
/// 24 discard, the 2 gap and the 20 stage toggle. The text always stops before
/// it, so neither the numbers nor the path run under an icon.
/// The hover verbs' zone: the 24 px discard, the 2 px gap and the 24 px
/// stage toggle.
const ROW_ACTIONS_W: f32 = 50.;

/// The file list's row height. Section headers match it, so `uniform_list`
/// can size the whole tree from one measured item.
const FILE_ROW_H: f32 = 34.;
/// The commit list's width.
const HISTORY_COLUMN_W: f32 = 320.;
/// The drilled commit's file rail, and the height of its group header and rows.
const COMMIT_RAIL_W: f32 = 240.;
const COMMIT_RAIL_ROW_H: f32 = 34.;
/// The git toolbar's height.
const TOOLBAR_H: f32 = 52.;
/// The file and commit lists' head height: the same band the work list uses.
const LIST_HEAD_H: f32 = 40.;
/// The branch selector's width.
const BRANCH_W: f32 = 260.;
/// The commit box's height: the message over its button.
const COMMIT_BOX_H: f32 = 190.;

pub struct ChangesView {
    pub store: Entity<Store>,
    /// The (path, staged) row the diff shows; its patch loads on selection.
    selected_file: Option<(String, bool)>,
    /// The persisted diff layout; global, not per repository.
    diff_view: DiffViewMode,
    /// Whether long diff rows wrap; persisted with the layout.
    diff_wrap: bool,
    /// Whether the file list draws its tree; persisted app-wide.
    tree_view: bool,
    /// The diff's horizontal scroll handles, kept so the pinned bars work.
    diff_scroll: DiffScrollHandles,
    file_by_repo: std::collections::HashMap<String, (String, bool)>,
    /// The row whose discard took its first click; the second one discards.
    discard_armed: Option<(String, bool)>,
    /// The unstaged header's discard-all has been clicked once: the second
    /// click throws the whole working tree away.
    discard_all_armed: bool,
    /// The file list's two sections fold independently; their headers and
    /// whole-tree verbs stay visible while the rows hide.
    staged_collapsed: bool,
    unstaged_collapsed: bool,
    patches: std::collections::HashMap<(String, bool), String>,
    /// The branch selector: the shared FilteredPicker in branch mode.
    branch_picker: Entity<FilteredPicker>,
    /// The branch picker's ephemeral state (the open form and the row menu);
    /// shared with the picker's closures.
    branch_ui: Rc<RefCell<BranchUi>>,
    /// The branch row menu's keyboard owner; focused while the menu is up so
    /// Escape takes it down.
    menu_focus: FocusHandle,
    commit_message: Entity<TextareaState>,
    history_open: bool,
    /// Hash of the drilled-in commit (None = the list view).
    drilled_commit: Option<String>,
    /// Armed destructive drill-down action ("checkout"/"revert") — the second
    /// click confirms the arm.
    detail_armed: Option<&'static str>,
    detail_patches: std::collections::HashMap<String, String>,
    /// Keys (`hash|path`) whose patch is already on the wire or landed; stops
    /// a rail selection from re-asking on every render.
    detail_requests: std::collections::HashSet<String>,
    /// The file the drilled commit's rail shows; reset when the drill moves.
    commit_file: usize,
    /// The rail's folded folders and cursor; the selection stays `commit_file`.
    commit_folded: std::collections::HashSet<String>,
    commit_cursor: Option<RailKey>,
    /// The rail's tree, built once per commit hash.
    commit_tree: Option<(String, std::sync::Arc<file_tree::Tree>)>,
    /// The rail's built items for (hash, tree view, rail revision).
    commit_items_cache: Option<(String, bool, u64, Rc<Vec<RailItem>>)>,
    commit_rail_revision: u64,
    /// The repo the view's local state belongs to; a mismatch resets it all.
    loaded_for: Option<String>,
    /// Staged/unstaged groups, rebuilt only when the store's change snapshot
    /// generation moves (any store notify otherwise re-cloned everything).
    groups_cache: Option<(u64, Rc<FileGroups>)>,
    /// Folded folder paths, shared by both sections; a repo change clears it.
    folded: std::collections::HashSet<String>,
    /// The row the list's cursor sits on, separate from the diff selection.
    file_cursor: Option<RowKey>,
    /// Bumped when the visible rows change (a fold, a section toggle), so the
    /// built item list knows to rebuild.
    list_revision: u64,
    /// The built item list for (snapshot generation, list revision): a render
    /// walks this, never the paths.
    items_cache: Option<(u64, u64, Rc<Vec<ChangeListItem>>)>,
    /// Precomputed history row labels, keyed by history generation + today.
    history_rows: Vec<HistoryRow>,
    history_cache_key: (u64, chrono::NaiveDate),
    /// The file list's keyboard focus.
    file_focus: FocusHandle,
    /// Keeps the keyboard-selected file row in view.
    file_scroll: UniformListScrollHandle,
    /// The history commit list's keyboard focus.
    history_focus: FocusHandle,
    /// The commit rail's own focus and scroll, for its tree's keys.
    commit_rail_focus: FocusHandle,
    commit_rail_scroll: UniformListScrollHandle,
    /// The height-measured commit list: day headers and commits as separate
    /// items, so the header never takes room from the commit below it.
    history_items: Vec<HistoryItem>,
    history_list: ListState,
    history_list_key: (u64, usize),
    /// The panes on screen this frame, shared with the page and the list
    /// views; this view registers whichever of its two lists it draws.
    pane_nav: Rc<pane_nav::PaneNav>,
}

impl ChangesView {
    /// A view with its own (empty) pane order: the standalone probes and a
    /// surface drawn outside the Repositories page use this.
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_pane_nav(store, Rc::new(pane_nav::PaneNav::new()), window, cx)
    }

    /// The page's view, sharing the pane order so `←`/`→` can cross from the
    /// repository list into this view's lists.
    pub fn with_pane_nav(
        store: Entity<Store>,
        pane_nav: Rc<pane_nav::PaneNav>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let new_branch_name = cx.new(|cx| InputState::new(window, cx).placeholder("branch name"));
        let branch_ui: Rc<RefCell<BranchUi>> = Rc::new(RefCell::new(BranchUi::default()));
        let menu_focus = cx.focus_handle();
        let branch_picker = build_branch_picker(
            &store,
            branch_ui.clone(),
            new_branch_name.clone(),
            &menu_focus,
            window,
            cx,
        );
        // the picker's open popup is appended by the toolbar, so picker-internal changes (filter typing, arming,
        // dismissal) must repaint this view too
        cx.observe(&branch_picker, |_, _, cx| cx.notify()).detach();
        let commit_message =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe your changes…"));
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        let diff_view = store.read(cx).diff_view();
        let diff_wrap = store.read(cx).diff_wrap();
        let tree_view = store.read(cx).tree_view();
        Self {
            store,
            selected_file: None,
            diff_view,
            diff_wrap,
            tree_view,
            diff_scroll: DiffScrollHandles::default(),
            file_by_repo: std::collections::HashMap::new(),
            discard_armed: None,
            discard_all_armed: false,
            staged_collapsed: false,
            unstaged_collapsed: false,
            patches: std::collections::HashMap::new(),
            branch_picker,
            branch_ui,
            menu_focus,
            commit_message,
            history_open: false,
            drilled_commit: None,
            detail_armed: None,
            detail_patches: std::collections::HashMap::new(),
            detail_requests: std::collections::HashSet::new(),
            commit_file: 0,
            commit_folded: std::collections::HashSet::new(),
            commit_cursor: None,
            commit_tree: None,
            commit_items_cache: None,
            commit_rail_revision: 0,
            loaded_for: None,
            groups_cache: None,
            folded: std::collections::HashSet::new(),
            file_cursor: None,
            list_revision: 0,
            items_cache: None,
            history_rows: Vec::new(),
            history_cache_key: (0, chrono::NaiveDate::MIN),
            file_focus: cx.focus_handle(),
            file_scroll: UniformListScrollHandle::default(),
            history_focus: cx.focus_handle(),
            commit_rail_focus: cx.focus_handle(),
            commit_rail_scroll: UniformListScrollHandle::default(),
            history_items: Vec::new(),
            history_list: ListState::new(0, ListAlignment::Top, px(400.)),
            history_list_key: (0, 0),
            pane_nav,
        }
    }

    /// One-time reset when the selected repo changed: the tab must never show
    /// the previous repo's draft, selection, menu or drill-down state.
    fn sync_repo_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repo = self.store.read(cx).selected_repo.clone();
        if self.loaded_for == repo {
            return;
        }
        self.loaded_for = repo;
        self.selected_file = None;
        self.discard_armed = None;
        self.discard_all_armed = false;
        self.staged_collapsed = false;
        self.unstaged_collapsed = false;
        self.patches.clear();
        self.branch_picker.update(cx, FilteredPicker::dismiss);
        {
            // the branch popup must never show the previous repo's list,
            // filter or form
            let mut ui = self.branch_ui.borrow_mut();
            ui.menu = None;
            ui.new_branch_open = false;
        }
        self.branch_picker
            .update(cx, |picker, cx| picker.clear_filter(window, cx));
        self.commit_message
            .update(cx, |f, cx| f.set_value("", window, cx));
        self.history_open = false;
        self.drilled_commit = None;
        self.detail_armed = None;
        self.detail_patches.clear();
        self.detail_requests.clear();
        self.commit_file = 0;
        self.reset_commit_rail();
        self.folded.clear();
        self.file_cursor = None;
        self.list_revision = self.list_revision.wrapping_add(1);
        self.items_cache = None;
    }

    /// Closes the branch picker from an outside click or Escape. Returns
    /// whether it was actually open so the window can stop propagating.
    pub fn dismiss_branch_menu(&mut self, cx: &mut Context<Self>) -> bool {
        self.branch_picker.update(cx, FilteredPicker::dismiss)
    }

    /// Folds or unfolds one of the file list's sections.
    fn toggle_section(&mut self, section: FileSection, cx: &mut Context<Self>) {
        match section {
            FileSection::Staged => self.staged_collapsed = !self.staged_collapsed,
            FileSection::Unstaged => self.unstaged_collapsed = !self.unstaged_collapsed,
        }
        // a folded section has no rows: the built item list must move with it
        self.list_revision = self.list_revision.wrapping_add(1);
        cx.notify();
    }

    /// Opens the history list, loading it once per selection.
    pub fn open_history(&mut self, cx: &mut Context<Self>) {
        let needs_load = self.store.read(cx).history().is_empty();
        self.history_open = true;
        self.drilled_commit = None;
        self.commit_file = 0;
        self.reset_commit_rail();
        if needs_load && let Some(folder) = self.store.read(cx).selected_repo.clone() {
            self.store.update(cx, |store, cx| {
                store.start_history_pass();
                cx.notify();
            });
            let _ = self
                .store
                .read(cx)
                .net
                .send_blocking(NetCommand::LoadHistory {
                    folder: std::path::PathBuf::from(folder),
                });
        }
        cx.notify();
    }

    /// Drills into one commit's detail.
    pub fn drill_commit(&mut self, hash: &str, cx: &mut Context<Self>) {
        self.drilled_commit = Some(hash.to_string());
        self.commit_file = 0;
        self.reset_commit_rail();
        if let Some(folder) = self.store.read(cx).selected_repo.clone() {
            let _ = self
                .store
                .read(cx)
                .net
                .send_blocking(NetCommand::GetCommitDetails {
                    folder: std::path::PathBuf::from(folder),
                    hash: hash.to_string(),
                });
        }
        cx.notify();
    }

    /// Asks for one commit file's patch, once per key.
    fn request_commit_patch(&mut self, hash: &str, path: &str, cx: &mut Context<Self>) {
        let key = format!("{hash}|{path}");
        if self.detail_patches.contains_key(&key) {
            return;
        }
        let Some(folder) = self.store.read(cx).selected_repo.clone() else {
            return;
        };
        if !self.detail_requests.insert(key) {
            return;
        }
        let _ = self
            .store
            .read(cx)
            .net
            .send_blocking(NetCommand::GetCommitFilePatch {
                folder: std::path::PathBuf::from(folder),
                hash: hash.to_string(),
                path: path.to_string(),
            });
    }

    /// Selects one file on the drilled commit's rail and loads its diff.
    fn select_commit_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.commit_file = index;
        let path = self
            .store
            .read(cx)
            .commit_details
            .as_ref()
            .and_then(|details| details.files.get(index))
            .map(|file| file.path.clone());
        if let Some(path) = path {
            self.commit_cursor = Some(RailKey {
                path,
                folder: false,
            });
        }
        self.ensure_commit_file_selected(cx);
        cx.notify();
    }

    /// Drops the rail's per-commit state: its cursor, its folds and its tree.
    fn reset_commit_rail(&mut self) {
        self.commit_cursor = None;
        self.commit_folded.clear();
        self.commit_tree = None;
        self.commit_items_cache = None;
        self.commit_rail_revision = self.commit_rail_revision.wrapping_add(1);
    }

    /// Keeps the drilled commit's rail selection on a live file: the first
    /// one stands in until the user picks another, and the patch it needs is
    /// put on the wire here (renders stay idempotent).
    fn ensure_commit_file_selected(&mut self, cx: &mut Context<Self>) {
        let Some(hash) = self.drilled_commit.clone() else {
            return;
        };
        let (detail_hash, path) = {
            let store = self.store.read(cx);
            let Some(details) = store.commit_details.as_ref() else {
                return;
            };
            if !details.hash.starts_with(&hash[..hash.len().min(7)]) {
                return;
            }
            if details.files.is_empty() {
                self.commit_file = 0;
                return;
            }
            if self.commit_file >= details.files.len() {
                self.commit_file = 0;
            }
            (
                details.hash.clone(),
                details.files[self.commit_file].path.clone(),
            )
        };
        self.request_commit_patch(&detail_hash, &path, cx);
    }

    /// The rail's tree for one commit, built once per commit hash.
    fn commit_rail_tree(
        &mut self,
        details: &warp_station_core::model::GitCommitDetails,
    ) -> std::sync::Arc<file_tree::Tree> {
        if let Some((hash, tree)) = &self.commit_tree
            && *hash == details.hash
        {
            return tree.clone();
        }
        let tree = std::sync::Arc::new(file_tree::Tree::build(
            details.files.iter().map(|file| file.path.as_str()),
        ));
        self.commit_tree = Some((details.hash.clone(), tree.clone()));
        self.commit_cursor = None;
        self.commit_folded.clear();
        self.commit_items_cache = None;
        self.commit_rail_revision = self.commit_rail_revision.wrapping_add(1);
        tree
    }

    /// The rail's visible items: the commit's tree, or the flat list when the
    /// app-wide switch is off.
    fn commit_rail_items(
        &mut self,
        details: &warp_station_core::model::GitCommitDetails,
    ) -> Rc<Vec<RailItem>> {
        let tree = self.commit_rail_tree(details);
        if let Some((hash, tree_view, revision, items)) = &self.commit_items_cache
            && *hash == details.hash
            && *tree_view == self.tree_view
            && *revision == self.commit_rail_revision
        {
            return items.clone();
        }
        let mut items = Vec::with_capacity(details.files.len() + 8);
        if self.tree_view {
            for row in tree.rows(&self.commit_folded) {
                items.push(match row {
                    file_tree::Row::Folder {
                        path,
                        label,
                        count,
                        depth,
                        ..
                    } => RailItem::Folder(FolderItem {
                        path: path.into(),
                        label: label.into(),
                        count,
                        depth,
                    }),
                    file_tree::Row::File { index, depth, .. } => RailItem::File { index, depth },
                });
            }
        } else {
            items.extend((0..details.files.len()).map(|index| RailItem::File { index, depth: 0 }));
        }
        let items = Rc::new(items);
        self.commit_items_cache = Some((
            details.hash.clone(),
            self.tree_view,
            self.commit_rail_revision,
            items.clone(),
        ));
        items
    }

    /// Every row the rail's cursor can land on, in item order.
    fn commit_rail_rows(&mut self, cx: &mut Context<Self>) -> Vec<RailRow> {
        let Some(details) = self.store.read(cx).commit_details.clone() else {
            return Vec::new();
        };
        let items = self.commit_rail_items(&details);
        let mut rows = Vec::with_capacity(items.len());
        for (item_ix, item) in items.iter().enumerate() {
            match item {
                RailItem::Folder(folder) => rows.push(RailRow {
                    item_ix,
                    depth: folder.depth,
                    key: RailKey {
                        path: folder.path.to_string(),
                        folder: true,
                    },
                    index: None,
                }),
                RailItem::File { index, depth } => {
                    let Some(file) = details.files.get(*index) else {
                        continue;
                    };
                    rows.push(RailRow {
                        item_ix,
                        depth: *depth,
                        key: RailKey {
                            path: file.path.clone(),
                            folder: false,
                        },
                        index: Some(*index),
                    });
                }
            }
        }
        rows
    }

    fn commit_cursor_index(&self, rows: &[RailRow]) -> Option<usize> {
        let cursor = self.commit_cursor.as_ref()?;
        rows.iter().position(|row| &row.key == cursor)
    }

    /// Steps the rail's cursor: a file row selects its diff, a folder row only
    /// moves the cursor.
    fn move_commit_cursor(&mut self, key: list_nav::ListKey, cx: &mut Context<Self>) {
        let rows = self.commit_rail_rows(cx);
        if rows.is_empty() {
            return;
        }
        let current = self.commit_cursor_index(&rows);
        let Some(next) = list_nav::step(current, rows.len(), key, list_nav::DEFAULT_PAGE) else {
            return;
        };
        let item_ix = rows[next].item_ix;
        let cursor = rows[next].key.clone();
        let index = rows[next].index;
        self.commit_cursor = Some(cursor);
        if let Some(index) = index {
            self.select_commit_file(index, cx);
        } else {
            cx.notify();
        }
        self.commit_rail_scroll
            .scroll_to_item(item_ix, ScrollStrategy::Nearest);
    }

    /// `Left` on a folder row of the rail: fold it, or step to its parent.
    fn collapse_commit_or_parent(&mut self, cx: &mut Context<Self>) -> bool {
        let rows = self.commit_rail_rows(cx);
        let Some(ix) = self.commit_cursor_index(&rows) else {
            return false;
        };
        let depth = rows[ix].depth;
        let row = &rows[ix];
        if !row.key.folder {
            return false;
        }
        if !self.commit_folded.contains(&row.key.path) {
            self.commit_folded.insert(row.key.path.clone());
            self.commit_rail_revision = self.commit_rail_revision.wrapping_add(1);
            cx.notify();
            return true;
        }
        if let Some(parent) = rows[..ix]
            .iter()
            .rev()
            .find(|candidate| candidate.depth < depth)
        {
            self.commit_cursor = Some(parent.key.clone());
            self.commit_rail_scroll
                .scroll_to_item(parent.item_ix, ScrollStrategy::Nearest);
            cx.notify();
        }
        true
    }

    /// `Right` on a folder row of the rail: unfold it, or step to its first
    /// child.
    fn expand_commit_or_child(&mut self, cx: &mut Context<Self>) -> bool {
        let rows = self.commit_rail_rows(cx);
        let Some(ix) = self.commit_cursor_index(&rows) else {
            return false;
        };
        let depth = rows[ix].depth;
        let row = &rows[ix];
        if !row.key.folder {
            return false;
        }
        if self.commit_folded.remove(&row.key.path) {
            self.commit_rail_revision = self.commit_rail_revision.wrapping_add(1);
            cx.notify();
            return true;
        }
        if let Some(child) = rows.get(ix + 1).filter(|candidate| candidate.depth > depth) {
            self.commit_cursor = Some(child.key.clone());
            self.commit_rail_scroll
                .scroll_to_item(child.item_ix, ScrollStrategy::Nearest);
            cx.notify();
        }
        true
    }

    /// `Enter` on a folder row of the rail folds or unfolds it; a file row
    /// stays unhandled.
    fn toggle_commit_folder_cursor(&mut self, cx: &mut Context<Self>) -> bool {
        let rows = self.commit_rail_rows(cx);
        let Some(ix) = self.commit_cursor_index(&rows) else {
            return false;
        };
        if !rows[ix].key.folder {
            return false;
        }
        let path = rows[ix].key.path.clone();
        self.toggle_commit_folder(&path, cx);
        true
    }

    fn toggle_commit_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.commit_folded.remove(path) {
            self.commit_folded.insert(path.to_string());
        }
        self.commit_rail_revision = self.commit_rail_revision.wrapping_add(1);
        cx.notify();
    }

    /// The rail's keys: a folder row owns `Left`/`Right`/`Enter`; the arrow
    /// walk crosses every visible row, and a file row leaves `Left`/`Right` to
    /// the pane move.
    fn handle_commit_rail_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.modified() {
            return false;
        }
        if let Some(key) = list_nav::list_key(event) {
            match key {
                list_nav::ListKey::Left => {
                    if self.collapse_commit_or_parent(cx) {
                        return true;
                    }
                }
                list_nav::ListKey::Right => {
                    if self.expand_commit_or_child(cx) {
                        return true;
                    }
                }
                list_nav::ListKey::Enter => return self.toggle_commit_folder_cursor(cx),
                list_nav::ListKey::Up
                | list_nav::ListKey::Down
                | list_nav::ListKey::Home
                | list_nav::ListKey::End
                | list_nav::ListKey::PageUp
                | list_nav::ListKey::PageDown => {
                    self.move_commit_cursor(key, cx);
                    return true;
                }
                _ => {}
            }
        }
        if let Some(dir) = pane_nav::direction(event) {
            return self
                .pane_nav
                .move_from(&self.commit_rail_focus, dir, window, cx);
        }
        false
    }

    /// Shows a file's diff, loading its patch the first time. A press selects
    /// the row.
    fn click_file(
        &mut self,
        path: &str,
        staged: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_file(path, staged, cx);
    }

    fn select_file(&mut self, path: &str, staged: bool, cx: &mut Context<Self>) {
        let key = (path.to_string(), staged);
        self.selected_file = Some(key.clone());
        self.file_cursor = Some(RowKey {
            path: path.to_string(),
            staged,
            folder: false,
        });
        if let Some(folder) = self.store.read(cx).selected_repo.clone() {
            self.file_by_repo.insert(folder, key);
        }
        self.request_change_patch(path, staged, cx);
        cx.notify();
    }

    /// Asks for one file's patch, once per file: a snapshot change re-asks
    /// after the patch cache is cleared, a re-selection asks nothing.
    fn request_change_patch(&mut self, path: &str, staged: bool, cx: &mut Context<Self>) {
        let key = (path.to_string(), staged);
        if self.patches.contains_key(&key) {
            return;
        }
        let Some(folder) = self.store.read(cx).selected_repo.clone() else {
            return;
        };
        let _ = self
            .store
            .read(cx)
            .net
            .send_blocking(NetCommand::GetChangePatch {
                folder: std::path::PathBuf::from(folder),
                path: path.to_string(),
                staged,
            });
    }

    /// Keeps the cursor on its row across a snapshot: a staged file moves to
    /// the staged tree, a folder that emptied falls away, and a row that left
    /// both lists leaves the cursor unset.
    fn reconcile_cursor(&mut self) {
        let Some(files) = self.groups_cache.as_ref().map(|(_, files)| files.clone()) else {
            return;
        };
        let Some(key) = self.file_cursor.clone() else {
            return;
        };
        let (here, there, here_tree, there_tree) = if key.staged {
            (
                &files.groups.0,
                &files.groups.1,
                &files.staged_tree,
                &files.unstaged_tree,
            )
        } else {
            (
                &files.groups.1,
                &files.groups.0,
                &files.unstaged_tree,
                &files.staged_tree,
            )
        };
        let present = if key.folder {
            here_tree.contains_folder(&key.path)
        } else {
            here.iter().any(|file| file.path == key.path)
        };
        if present {
            return;
        }
        let moved = if key.folder {
            there_tree.contains_folder(&key.path)
        } else {
            there.iter().any(|file| file.path == key.path)
        };
        self.file_cursor = moved.then_some(RowKey {
            staged: !key.staged,
            ..key
        });
    }

    /// Follows the selected file into the other section when a snapshot moved
    /// it, so staging (or unstaging) a file keeps the diff on that path instead
    /// of jumping to the section's first file.
    fn follow_selected_file(&mut self, files: &FileGroups, cx: &mut Context<Self>) {
        let Some((path, staged)) = self.selected_file.clone() else {
            return;
        };
        let exists = |side: bool| {
            let side = if side {
                &files.groups.0
            } else {
                &files.groups.1
            };
            side.iter().any(|file| file.path == path)
        };
        if exists(staged) || !exists(!staged) {
            return;
        }
        let moved = (path, !staged);
        self.selected_file = Some(moved.clone());
        // the repository remembers the row it was left on
        if let Some(folder) = self.store.read(cx).selected_repo.clone() {
            self.file_by_repo.insert(folder, moved);
        }
    }

    /// Keeps a live row selected: the first file stands in until the user picks
    /// another, and again when the selected one leaves the lists.
    fn ensure_file_selected(&mut self, groups: &GroupedFiles, cx: &mut Context<Self>) {
        let (staged, unstaged) = groups;
        let live = self
            .selected_file
            .as_ref()
            .is_some_and(|(path, is_staged)| {
                let side = if *is_staged { staged } else { unstaged };
                side.iter().any(|file| &file.path == path)
            });
        if live {
            return;
        }
        self.selected_file = None;
        // each repository keeps the file it was left on
        let kept = self
            .store
            .read(cx)
            .selected_repo
            .as_ref()
            .and_then(|folder| self.file_by_repo.get(folder))
            .filter(|(path, is_staged)| {
                let side = if *is_staged { staged } else { unstaged };
                side.iter().any(|file| &file.path == path)
            })
            .cloned();
        let first = kept
            .or_else(|| unstaged.first().map(|file| (file.path.clone(), false)))
            .or_else(|| staged.first().map(|file| (file.path.clone(), true)));
        if let Some((path, is_staged)) = first {
            self.select_file(&path, is_staged, cx);
        }
    }
}

impl Render for ChangesView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.sync_repo_reset(window, cx);
        // the stopped operation's session feeds both the conflict banner and
        // the merge editor; loading it here keeps them on one answer
        self.store
            .update(cx, |store, cx| store.ensure_merge_loaded(cx));
        // the switch is app-wide: a change from the head must reach the rows on
        // this same draw
        let tree_view = self.store.read(cx).tree_view();
        if tree_view != self.tree_view {
            self.tree_view = tree_view;
            self.list_revision = self.list_revision.wrapping_add(1);
        }
        // the History tab and the Changes tab are this one view
        let want_history =
            self.store.read(cx).active_detail_tab() == crate::store::WorkspaceTab::History;
        if want_history != self.history_open {
            if want_history {
                self.open_history(cx);
            } else {
                self.history_open = false;
                self.drilled_commit = None;
            }
        }
        // drain freshly loaded patches + the commit wand's draft; patches are
        // \r-stripped once here so the diff renderer never rewrites the text
        // per render
        if let Some((path, staged, patch)) = self
            .store
            .update(cx, |store, _| store.change_patch_result.take())
        {
            self.patches.insert((path, staged), patch.replace('\r', ""));
        }
        // a settled commit empties the message box (before the draft drain so
        // a wand draft landing in the same pass survives)
        if self.store.update(cx, |store, _| {
            std::mem::take(&mut store.commit_message_clear)
        }) {
            self.commit_message
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        if let Some(draft) = self
            .store
            .update(cx, |store, _| store.commit_message_draft.take())
        {
            let draft = draft.trim().to_string();
            self.commit_message
                .update(cx, |state, cx| state.set_value(draft.as_str(), window, cx));
        }
        for (key, patch) in self.store.update(cx, |store, _| {
            std::mem::take(&mut store.commit_file_patches)
        }) {
            self.detail_requests.remove(&key);
            self.detail_patches.insert(key, patch.replace('\r', ""));
        }

        let (generation, busy) = {
            let store = self.store.read(cx);
            (store.changes_generation(), store.git_busy())
        };
        if self
            .groups_cache
            .as_ref()
            .is_none_or(|(g, _)| *g != generation)
        {
            // the snapshot is cloned into the groups anyway; cloning it once
            // more first would copy a ten-thousand-file tree for nothing
            let groups = {
                let store = self.store.read(cx);
                store
                    .changes()
                    .map(|snapshot| snapshot.change_groups())
                    .unwrap_or_default()
            };
            let unstaged_paths = Rc::new(
                groups
                    .1
                    .iter()
                    .map(|file| file.path.clone())
                    .collect::<Vec<_>>(),
            );
            let staged_tree =
                file_tree::Tree::build(groups.0.iter().map(|file| file.path.as_str()));
            let unstaged_tree =
                file_tree::Tree::build(groups.1.iter().map(|file| file.path.as_str()));
            self.groups_cache = Some((
                generation,
                Rc::new(FileGroups {
                    groups,
                    unstaged_paths,
                    staged_tree,
                    unstaged_tree,
                }),
            ));
            let snapshot = self.groups_cache.as_ref().map(|(_, files)| files.clone());
            if let Some(files) = &snapshot {
                self.follow_selected_file(files, cx);
            }
            self.items_cache = None;
            self.reconcile_cursor();
            // a new snapshot can add or drop files under an armed discard-all,
            // so the arming never outlives the tree it was given for
            self.discard_all_armed = false;
            self.discard_armed = None;
            // a new snapshot means the fetched patches may be stale
            self.patches.clear();
            if let Some((path, staged)) = self.selected_file.clone() {
                self.request_change_patch(&path, staged, cx);
            }
        }
        let file_groups = self.groups_cache.as_ref().expect("groups cached").1.clone();
        let groups = &file_groups.groups;
        self.ensure_file_selected(groups, cx);
        self.ensure_commit_file_selected(cx);
        if self.history_open {
            self.pane_nav.register(&self.history_focus);
            return div().size_full().flex().flex_col().min_h_0().child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_history(window, cx)),
            );
        }

        let toolbar = self.render_toolbar(busy, cx);
        let popup = self
            .branch_picker
            .update(cx, |picker, cx| picker.overlay_popup(window, cx));
        // A clean tree is not a file table with nothing in it: it is one
        // statement, so it takes the body and the commit box steps aside
        // until there is something to commit (the design's empty state).
        let clean = groups.0.is_empty()
            && groups.1.is_empty()
            && self.store.read(cx).changes_loaded_for.is_some();
        // only a staged file can be committed: an unstaged-only tree keeps the
        // file list but grows no commit box (a commit here could only fail)
        let committing = self.store.read(cx).running_job() == Some("commit");
        let body = if clean {
            clean_state(&self.store, cx)
        } else {
            self.pane_nav.register(&self.file_focus);
            let file_list = self.render_file_list(&file_groups, cx);
            // The list owns the row keys only while its handle holds the
            // focus; its head says so with the label's ink and its rule.
            let file_focused = self.file_focus.is_focused(window);
            let file_count = groups.0.len() + groups.1.len();
            let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
            let mut files = div()
                .w(px(warpui_kit_core::shell::scaled(FILE_COLUMN_W)))
                .flex_shrink_0()
                .h_full()
                .min_h_0()
                .flex()
                .flex_col()
                .bg(theme::surface())
                .border_r_1()
                .border_color(theme::hairline())
                .child(
                    // the rows take the keys only while this column owns the
                    // focus; the commit card below is an input and keeps its
                    // own
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .track_focus(&self.file_focus)
                        .on_key_down(cx.listener(|this, event, window, cx| {
                            if this.handle_file_key(event, window, cx) {
                                cx.stop_propagation();
                            }
                        }))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                if !this.file_focus.is_focused(window) {
                                    window.focus(&this.file_focus, cx);
                                }
                            }),
                        )
                        .child(Self::list_head(
                            "Files",
                            file_count,
                            file_focused,
                            "files-head-focused",
                            "files-head-idle",
                            Some(self.render_tree_toggle(show_tooltips, cx)),
                        ))
                        .child(file_list),
                );
            files = if groups.0.is_empty() && !committing {
                files.child(stage_hint(self.store.clone(), groups.1.len()))
            } else {
                files.child(self.render_commit_card(busy, window, cx))
            };
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(files)
                .child(self.render_diff(window, cx))
                .into_any_element()
        };
        // the branch popup hangs off the toolbar's own bottom edge: the
        // toolbar no longer shifts the panel away from its trigger
        let toolbar_and_popup =
            div()
                .relative()
                .flex_shrink_0()
                .child(toolbar)
                .children(popup.map(|popup| {
                    gpui::deferred(
                        div()
                            .absolute()
                            .top(px(TOOLBAR_H))
                            .left(px(8.))
                            .w(px(BRANCH_W))
                            .child(popup),
                    )
                }));
        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(toolbar_and_popup)
            .child(body)
            // the row menus paint last: they are the tab's topmost layer
            // while they are up
            .children(self.render_branch_menu(cx))
    }
}

impl ChangesView {
    /// The selected file's diff.
    fn render_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(key) = self.selected_file.clone() else {
            return div()
                .id("changes-diff")
                .debug_selector(|| "changes-diff".into())
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(theme::well())
                .into_any_element();
        };
        let patch = self.patches.get(&key).map(String::as_str);
        self.render_diff_pane(
            "changes-diff",
            "changes-diff-menu",
            &key.0,
            patch,
            window,
            cx,
        )
    }

    /// The Changes and History tabs' shared diff pane: the 44-high path bar
    /// with the persisted wrap/layout toggles over the selectable diff body.
    /// `None` is a patch still on the wire, `Some("")` a settled answer with
    /// nothing textual to draw (binary or mode-only).
    fn render_diff_pane(
        &self,
        pane_selector: &'static str,
        menu_name: &'static str,
        path: &str,
        patch: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let split = self.diff_view == DiffViewMode::Split;
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        // a conflicted path has no index-vs-worktree patch, so the frame's
        // "No textual changes" would be a dead end: offer the editor instead
        let merge_file = self.store.read(cx).merge_file_for(path).cloned();
        // an empty patch is a state, not a one-row diff: a binary file or a
        // mode-only change has nothing textual to draw
        let has_patch = merge_file.is_none() && matches!(patch, Some(patch) if !patch.is_empty());
        let body: gpui::AnyElement = match merge_file.as_ref() {
            Some(file) => merge_conflict_state(file, self.store.clone()),
            None => match patch {
                Some(patch) if !patch.is_empty() => div()
                    .id(SharedString::from(format!("{pane_selector}-scroll")))
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(diff_controls::attach_filling(
                        gpui::ElementId::Name(menu_name.into()),
                        render_diff_with_scroll(
                            patch,
                            None,
                            warpui_kit_diff::DiffOptions {
                                mode: diff_controls::kit_mode(self.diff_view),
                                wrap: self.diff_wrap,
                            },
                            &self.diff_scroll,
                            theme::font_mono_meta(),
                            window,
                            cx,
                        ),
                        warpui_kit_core::text_menu::focus(cx),
                    ))
                    .into_any_element(),
                Some(_) => diff_controls::state("No textual changes"),
                None => diff_controls::state("Loading patch…"),
            },
        };
        let wrap_view = cx.entity();
        let layout_view = wrap_view.clone();
        div()
            .id(pane_selector)
            .debug_selector(move || pane_selector.to_string())
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::well())
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(44.))
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .bg(theme::panel())
                    .border_b_1()
                    .border_color(theme::hairline())
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .text_color(theme::text())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(path.to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(diff_controls::wrap_toggle(
                                "diff",
                                self.diff_wrap,
                                self.store.clone(),
                                move |next, cx| {
                                    wrap_view.update(cx, |this, cx| {
                                        this.diff_wrap = next;
                                        cx.notify();
                                    });
                                },
                                show_tooltips,
                            ))
                            .child(diff_controls::view_toggle(
                                "diff",
                                split,
                                self.store.clone(),
                                move |next, cx| {
                                    layout_view.update(cx, |this, cx| {
                                        this.diff_view = next;
                                        cx.notify();
                                    });
                                },
                                show_tooltips,
                            )),
                    ),
            )
            .child(body)
            .children((has_patch && !self.diff_wrap).then(|| {
                diff_controls::scrollbars(
                    "diff",
                    &self.diff_scroll,
                    self.diff_view == DiffViewMode::Split,
                )
            }))
            .into_any_element()
    }

    /// The head's tree/flat switch: one app-wide setting, so every file list
    /// follows the same choice.
    fn render_tree_toggle(
        &mut self,
        show_tooltips: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let tree = self.tree_view;
        let button = div()
            .id("files-tree-toggle")
            .debug_selector(|| "files-tree-toggle".into())
            .a11y_toggle("Tree view", tree)
            .cursor_pointer()
            .rounded_md()
            .p_1()
            .flex()
            .items_center()
            .when(tree, |d| d.bg(theme::raised()))
            .hover(|d| d.bg(theme::raised()))
            .on_click(cx.listener(|this, _, _window, cx| this.toggle_tree_view(cx)))
            .child(icon(
                if tree {
                    "icons/icon-folder.svg"
                } else {
                    "icons/icon-list-bullet.svg"
                },
                px(theme::icon_small()),
                if tree {
                    theme::text()
                } else {
                    theme::text_muted()
                },
            ));
        crate::ui::with_tooltip(button, show_tooltips, "Tree view").into_any_element()
    }

    /// Flips the app-wide file-list switch and persists it for every list.
    fn toggle_tree_view(&mut self, cx: &mut Context<Self>) {
        let next = !self.tree_view;
        self.tree_view = next;
        self.list_revision = self.list_revision.wrapping_add(1);
        self.store.update(cx, |store, _| store.set_tree_view(next));
        cx.notify();
    }

    /// The file list flattened to its fixed-height items, in render order.
    /// The renderer and the keyboard cursor both read this one order, so a
    /// collapsed section or a new snapshot moves them together.
    fn file_items(&self, files: &FileGroups) -> Vec<ChangeListItem> {
        if !self.tree_view {
            return self.flat_items(files);
        }
        let mut items: Vec<ChangeListItem> =
            Vec::with_capacity(files.groups.0.len() + files.groups.1.len() + 4);
        self.section_items(true, &files.staged_tree, files.groups.0.len(), &mut items);
        self.section_items(
            false,
            &files.unstaged_tree,
            files.groups.1.len(),
            &mut items,
        );
        items
    }

    /// One section's header and visible tree rows.
    fn section_items(
        &self,
        staged: bool,
        tree: &file_tree::Tree,
        count: usize,
        items: &mut Vec<ChangeListItem>,
    ) {
        if count == 0 {
            return;
        }
        items.push(if staged {
            ChangeListItem::StagedHeader
        } else {
            ChangeListItem::UnstagedHeader
        });
        let collapsed = if staged {
            self.staged_collapsed
        } else {
            self.unstaged_collapsed
        };
        if collapsed {
            return;
        }
        for row in tree.rows(&self.folded) {
            items.push(match row {
                file_tree::Row::Folder {
                    path,
                    label,
                    count,
                    depth,
                    ..
                } => {
                    let folder = FolderItem {
                        path: path.into(),
                        label: label.into(),
                        count,
                        depth,
                    };
                    if staged {
                        ChangeListItem::StagedFolder(folder)
                    } else {
                        ChangeListItem::UnstagedFolder(folder)
                    }
                }
                file_tree::Row::File { index, depth, .. } => {
                    if staged {
                        ChangeListItem::StagedFile { index, depth }
                    } else {
                        ChangeListItem::UnstagedFile { index, depth }
                    }
                }
            });
        }
    }

    /// The flat list: today's rows, in snapshot order, with no folder rows.
    fn flat_items(&self, files: &FileGroups) -> Vec<ChangeListItem> {
        let mut items: Vec<ChangeListItem> =
            Vec::with_capacity(files.groups.0.len() + files.groups.1.len() + 2);
        for (staged, count, collapsed) in [
            (true, files.groups.0.len(), self.staged_collapsed),
            (false, files.groups.1.len(), self.unstaged_collapsed),
        ] {
            if count == 0 {
                continue;
            }
            items.push(if staged {
                ChangeListItem::StagedHeader
            } else {
                ChangeListItem::UnstagedHeader
            });
            if collapsed {
                continue;
            }
            items.extend((0..count).map(|index| {
                if staged {
                    ChangeListItem::StagedFile { index, depth: 0 }
                } else {
                    ChangeListItem::UnstagedFile { index, depth: 0 }
                }
            }));
        }
        items
    }

    /// The built item list, rebuilt only when the snapshot or the fold set
    /// moves: a ten-thousand-file tree is never re-walked per frame.
    fn list_items(&mut self, files: &FileGroups, generation: u64) -> Rc<Vec<ChangeListItem>> {
        if let Some((cached_generation, cached_revision, items)) = &self.items_cache
            && *cached_generation == generation
            && *cached_revision == self.list_revision
        {
            return items.clone();
        }
        let items = Rc::new(self.file_items(files));
        self.items_cache = Some((generation, self.list_revision, items.clone()));
        items
    }

    /// Every row the cursor can land on, in item order.
    fn cursor_rows(&mut self, cx: &mut Context<Self>) -> Vec<CursorRow> {
        let Some(files) = self.groups_cache.as_ref().map(|(_, files)| files.clone()) else {
            return Vec::new();
        };
        let generation = self.store.read(cx).changes_generation();
        let items = self.list_items(&files, generation);
        let mut rows = Vec::with_capacity(items.len());
        for (item_ix, item) in items.iter().enumerate() {
            match item {
                ChangeListItem::StagedHeader | ChangeListItem::UnstagedHeader => {}
                ChangeListItem::StagedFolder(folder) => {
                    rows.push(CursorRow {
                        item_ix,
                        depth: folder.depth,
                        key: RowKey {
                            path: folder.path.to_string(),
                            staged: true,
                            folder: true,
                        },
                    });
                }
                ChangeListItem::UnstagedFolder(folder) => {
                    rows.push(CursorRow {
                        item_ix,
                        depth: folder.depth,
                        key: RowKey {
                            path: folder.path.to_string(),
                            staged: false,
                            folder: true,
                        },
                    });
                }
                ChangeListItem::StagedFile { index, depth } => rows.push(CursorRow {
                    item_ix,
                    depth: *depth,
                    key: RowKey {
                        path: files.groups.0[*index].path.clone(),
                        staged: true,
                        folder: false,
                    },
                }),
                ChangeListItem::UnstagedFile { index, depth } => rows.push(CursorRow {
                    item_ix,
                    depth: *depth,
                    key: RowKey {
                        path: files.groups.1[*index].path.clone(),
                        staged: false,
                        folder: false,
                    },
                }),
            }
        }
        rows
    }

    fn cursor_row_index(&self, rows: &[CursorRow]) -> Option<usize> {
        let cursor = self.file_cursor.as_ref()?;
        rows.iter().position(|row| &row.key == cursor)
    }

    /// `Left` on a folder row: fold it, or step to its parent when it is
    /// already folded. A file row leaves `Left` to the pane move.
    fn collapse_or_parent(&mut self, cx: &mut Context<Self>) -> bool {
        let rows = self.cursor_rows(cx);
        let Some(ix) = self.cursor_row_index(&rows) else {
            return false;
        };
        let depth = rows[ix].depth;
        let row = &rows[ix];
        if !row.key.folder {
            return false;
        }
        if !self.folded.contains(&row.key.path) {
            self.folded.insert(row.key.path.clone());
            self.list_revision = self.list_revision.wrapping_add(1);
            cx.notify();
            return true;
        }
        if let Some(parent) = rows[..ix]
            .iter()
            .rev()
            .find(|candidate| candidate.depth < depth)
        {
            self.file_cursor = Some(parent.key.clone());
            self.file_scroll
                .scroll_to_item(parent.item_ix, ScrollStrategy::Nearest);
            cx.notify();
        }
        true
    }

    /// `Right` on a folder row: unfold it, or step to its first child when it
    /// is already open. A file row leaves `Right` to the pane move.
    fn expand_or_child(&mut self, cx: &mut Context<Self>) -> bool {
        let rows = self.cursor_rows(cx);
        let Some(ix) = self.cursor_row_index(&rows) else {
            return false;
        };
        let depth = rows[ix].depth;
        let row = &rows[ix];
        if !row.key.folder {
            return false;
        }
        if self.folded.remove(&row.key.path) {
            self.list_revision = self.list_revision.wrapping_add(1);
            cx.notify();
            return true;
        }
        if let Some(child) = rows.get(ix + 1).filter(|candidate| candidate.depth > depth) {
            self.file_cursor = Some(child.key.clone());
            self.file_scroll
                .scroll_to_item(child.item_ix, ScrollStrategy::Nearest);
            cx.notify();
        }
        true
    }

    /// `Enter` on a folder row folds or unfolds it; on a file row it stays
    /// unhandled, as before.
    fn toggle_cursor_folder(&mut self, cx: &mut Context<Self>) -> bool {
        let rows = self.cursor_rows(cx);
        let Some(ix) = self.cursor_row_index(&rows) else {
            return false;
        };
        if !rows[ix].key.folder {
            return false;
        }
        let path = rows[ix].key.path.clone();
        self.toggle_folder(&path, cx);
        true
    }

    /// Folds or unfolds one folder, keeping the built item list in step.
    fn toggle_folder(&mut self, path: &str, cx: &mut Context<Self>) {
        if !self.folded.remove(path) {
            self.folded.insert(path.to_string());
        }
        self.list_revision = self.list_revision.wrapping_add(1);
        cx.notify();
    }

    /// A folder row's press: put the cursor on the row and fold it, never
    /// touching the diff.
    fn click_folder(&mut self, folder: &FolderItem, staged: bool, cx: &mut Context<Self>) {
        self.file_cursor = Some(RowKey {
            path: folder.path.to_string(),
            staged,
            folder: true,
        });
        self.toggle_folder(&folder.path, cx);
    }

    /// The paths a folder row's verbs act on: its subtree in its own section.
    fn folder_paths(&self, path: &str, staged: bool) -> Vec<String> {
        let Some(files) = self.groups_cache.as_ref().map(|(_, files)| files.clone()) else {
            return Vec::new();
        };
        let (tree, side) = if staged {
            (&files.staged_tree, &files.groups.0)
        } else {
            (&files.unstaged_tree, &files.groups.1)
        };
        tree.files_under(path)
            .into_iter()
            .filter_map(|index| side.get(index).map(|file| file.path.clone()))
            .collect()
    }

    /// Steps the cursor through the visible rows: headers are skipped, and a
    /// folder row takes the cursor without touching the diff.
    fn move_file_selection(&mut self, key: list_nav::ListKey, cx: &mut Context<Self>) {
        let rows = self.cursor_rows(cx);
        if rows.is_empty() {
            return;
        }
        let current = self.cursor_row_index(&rows);
        let Some(next) = list_nav::step(current, rows.len(), key, list_nav::DEFAULT_PAGE) else {
            return;
        };
        let (item_ix, key_row) = (rows[next].item_ix, rows[next].key.clone());
        self.file_cursor = Some(key_row.clone());
        if key_row.folder {
            cx.notify();
        } else {
            self.select_file(&key_row.path, key_row.staged, cx);
        }
        self.file_scroll
            .scroll_to_item(item_ix, ScrollStrategy::Nearest);
    }

    /// The file list's keys, dispatched by the focused list only.
    /// The file list's keys, dispatched by the focused list only. A folder row
    /// owns `Left`/`Right`/`Enter`; a file row leaves `Left`/`Right` to the
    /// pane move, so the workbench's lists still walk with the arrows.
    fn handle_file_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.modified() || self.history_open {
            return false;
        }
        if let Some(key) = list_nav::list_key(event) {
            match key {
                list_nav::ListKey::Left => {
                    if self.collapse_or_parent(cx) {
                        return true;
                    }
                }
                list_nav::ListKey::Right => {
                    if self.expand_or_child(cx) {
                        return true;
                    }
                }
                list_nav::ListKey::Enter => return self.toggle_cursor_folder(cx),
                list_nav::ListKey::Up
                | list_nav::ListKey::Down
                | list_nav::ListKey::Home
                | list_nav::ListKey::End
                | list_nav::ListKey::PageUp
                | list_nav::ListKey::PageDown => {
                    self.move_file_selection(key, cx);
                    return true;
                }
                _ => {}
            }
        }
        if let Some(dir) = pane_nav::direction(event) {
            return self.pane_nav.move_from(&self.file_focus, dir, window, cx);
        }
        false
    }

    /// The history list's keys, dispatched by the focused list only.
    fn handle_history_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(dir) = pane_nav::direction(event) {
            return self
                .pane_nav
                .move_from(&self.history_focus, dir, window, cx);
        }
        if event.keystroke.modifiers.modified() || !self.history_open {
            return false;
        }
        let Some(key) = list_nav::list_key(event) else {
            return false;
        };
        match key {
            list_nav::ListKey::Enter => false,
            list_nav::ListKey::Up
            | list_nav::ListKey::Down
            | list_nav::ListKey::Home
            | list_nav::ListKey::End
            | list_nav::ListKey::PageUp
            | list_nav::ListKey::PageDown => {
                self.move_history_selection(key, cx);
                true
            }
            _ => false,
        }
    }

    /// Drills the next commit: the drilled row is this list's cursor, and the
    /// store loads its files and head band as it moves.
    fn move_history_selection(&mut self, key: list_nav::ListKey, cx: &mut Context<Self>) {
        let current = self
            .drilled_commit
            .as_ref()
            .and_then(|hash| self.history_rows.iter().position(|row| &row.hash == hash));
        let Some(next) = list_nav::step(
            current,
            self.history_rows.len(),
            key,
            list_nav::DEFAULT_PAGE,
        ) else {
            return;
        };
        let hash = self.history_rows[next].hash.clone();
        self.drill_commit(&hash, cx);
        if let Some(item) = self
            .history_items
            .iter()
            .position(|item| matches!(item, HistoryItem::Commit(index) if *index == next))
        {
            self.history_list.scroll_to_reveal_item(item);
        }
    }

    /// A list column's head: its name and row count, dim until the list owns
    /// the keys. The bottom rule is one pixel in both states and only changes
    /// color, so focusing the list never shifts a row.
    fn list_head(
        label: &'static str,
        count: usize,
        focused: bool,
        focused_selector: &'static str,
        idle_selector: &'static str,
        trailing: Option<gpui::AnyElement>,
    ) -> impl IntoElement {
        div()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(LIST_HEAD_H))
            .px_4()
            .border_b_1()
            .border_color(if focused {
                theme::accent()
            } else {
                theme::hairline()
            })
            .text_size(px(theme::font_meta()))
            .text_color(if focused {
                theme::text()
            } else {
                theme::text_dim()
            })
            .debug_selector(move || {
                if focused {
                    focused_selector.to_string()
                } else {
                    idle_selector.to_string()
                }
            })
            .child(label)
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::text_muted())
                    .child(count.to_string()),
            )
            .children(trailing.map(|trailing| div().ml_auto().child(trailing)))
    }

    fn render_file_list(
        &mut self,
        files: &Rc<FileGroups>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (staged, unstaged) = (&files.groups.0, &files.groups.1);

        if staged.is_empty() && unstaged.is_empty() {
            let loaded = self.store.read(cx).changes_loaded_for.is_some();
            return div()
                .id("changes-list")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .py_1()
                .child(if loaded {
                    clean_state(&self.store, cx)
                } else {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::text_dim())
                        .child(
                            div()
                                .id("changes-loading-status")
                                .role(gpui::Role::Status)
                                .aria_label("Loading changes")
                                .tooltip(crate::ui::tooltip_text("Loading changes"))
                                .flex()
                                .items_center()
                                .child(motion::spinner(
                                    "changes-loading",
                                    "icons/icon-spinner.svg",
                                    px(theme::icon_tiny()),
                                    theme::text_dim(),
                                )),
                        )
                        .child("Loading…")
                        .into_any_element()
                })
                .into_any_element();
        }

        // Headers and rows flatten into one fixed-height item list, and
        // `uniform_list` builds only the items in the viewport: a 10k-file
        // tree costs the same per repaint as a 30-file one. The old loop laid
        // out every row — its status letter, its dir/name split, its two
        // hover actions and their listeners — on every frame.
        let generation = self.store.read(cx).changes_generation();
        let items = self.list_items(files, generation);
        let files = files.clone();
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        uniform_list(
            SharedString::from("changes-list"),
            items.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let entity = cx.entity();
                let (staged, unstaged) = (&files.groups.0, &files.groups.1);
                range
                    .map(|ix| match items[ix].clone() {
                        ChangeListItem::StagedHeader => section_header(
                            FileSection::Staged,
                            staged.len(),
                            this.staged_collapsed,
                            entity.clone(),
                            vec![staging_icon(
                                "stage-all-clear",
                                "icons/icon-minus.svg",
                                "Unstage all",
                                GitAction::UnstageAll,
                                this.store.clone(),
                                show_tooltips,
                            )],
                        )
                        .into_any_element(),
                        ChangeListItem::StagedFolder(folder) => {
                            this.render_folder_row(&folder, true, cx)
                        }
                        ChangeListItem::StagedFile { index, depth } => {
                            this.render_row(&staged[index], true, depth, cx)
                        }
                        ChangeListItem::UnstagedHeader => section_header(
                            FileSection::Unstaged,
                            unstaged.len(),
                            this.unstaged_collapsed,
                            entity.clone(),
                            vec![
                                discard_all_button(
                                    this.discard_all_armed,
                                    files.unstaged_paths.clone(),
                                    this.store.clone(),
                                    entity.clone(),
                                    show_tooltips,
                                ),
                                staging_icon(
                                    "stage-all",
                                    "icons/icon-plus.svg",
                                    "Stage all",
                                    GitAction::StageAll,
                                    this.store.clone(),
                                    show_tooltips,
                                ),
                            ],
                        )
                        .into_any_element(),
                        ChangeListItem::UnstagedFolder(folder) => {
                            this.render_folder_row(&folder, false, cx)
                        }
                        ChangeListItem::UnstagedFile { index, depth } => {
                            this.render_row(&unstaged[index], false, depth, cx)
                        }
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .py_1()
        .track_scroll(&self.file_scroll)
        .debug_selector(|| "changes-list".into())
        .into_any_element()
    }

    /// A folder row: chevron, compact name, file count. A press folds it and
    /// puts the cursor on it; the diff keeps its file.
    fn render_folder_row(
        &mut self,
        folder: &FolderItem,
        staged: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let folded = self.folded.contains(folder.path.as_ref());
        let cursor = self.file_cursor.as_ref()
            == Some(&RowKey {
                path: folder.path.to_string(),
                staged,
                folder: true,
            });
        let row_id: SharedString =
            format!("{}-folder-{}", if staged { "s" } else { "u" }, folder.path).into();
        let row_selector = row_id.clone();
        let label: SharedString = format!(
            "{} ({} {})",
            folder.label,
            folder.count,
            if folder.count == 1 { "file" } else { "files" }
        )
        .into();
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        let armed =
            !staged && self.discard_armed.as_ref() == Some(&(folder.path.to_string(), false));
        let entity = cx.entity();
        let staging = folder_staging_button(
            staged,
            folder.path.as_ref(),
            folder.label.as_ref(),
            folder.count,
            entity.clone(),
            self.store.clone(),
            show_tooltips,
        );
        let discard = (!staged).then(|| {
            folder_discard_button(
                armed,
                folder.path.as_ref(),
                folder.label.as_ref(),
                folder.count,
                entity,
                self.store.clone(),
                show_tooltips,
            )
        });
        let click_folder = folder.clone();
        div()
            .id(row_id)
            .debug_selector(move || row_selector.to_string())
            .a11y_tree_item(label, Some(!folded))
            .group("file-row")
            .cursor_pointer()
            .w_full()
            .h(px(FILE_ROW_H))
            .pl(px(13. + folder.depth as f32 * 14.))
            .pr_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_l_3()
            .border_color(theme::surface())
            .when(cursor, |d| d.bg(theme::raised()))
            .hover(|s| s.bg(theme::panel()))
            .on_click(cx.listener(move |this, _, _window, cx| {
                this.click_folder(&click_folder, staged, cx);
            }))
            .child(icon(
                if folded {
                    "icons/icon-chevron-right.svg"
                } else {
                    "icons/icon-chevron-down.svg"
                },
                px(theme::icon_tiny()),
                theme::text_faint(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(theme::font_row_meta()))
                    .text_color(theme::text())
                    .child(folder.label.to_string()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .min_w(px(ROW_ACTIONS_W))
                    .flex()
                    .justify_end()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::text_muted())
                    .group_hover("file-row", |s| s.invisible())
                    .child(folder.count.to_string()),
            )
            .child(
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_0p5()
                    .when(!armed, |d| {
                        d.invisible().group_hover("file-row", |s| s.visible())
                    })
                    .children(discard)
                    .child(staging),
            )
            .into_any_element()
    }

    fn render_row(
        &mut self,
        file: &GitChangedFile,
        staged: bool,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = (file.path.clone(), staged);
        let selected = self.selected_file.as_ref() == Some(&key);
        let armed = self.discard_armed.as_ref() == Some(&key);
        let status_color = diff_controls::status_color(file.status_code.as_str());
        let row_id: SharedString =
            format!("{}-{}", if staged { "s" } else { "u" }, file.path).into();
        let row_selector = row_id.clone();
        // the tree's folder rows carry the directory; the flat list shows it
        // beside the name, as it always has
        let (dir, name) = if self.tree_view {
            let name = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(file.path.as_str())
                .to_string();
            (String::new(), name)
        } else {
            match file.path.rsplit_once('/') {
                Some((dir, name)) => (dir.to_string(), name.to_string()),
                None => (String::new(), file.path.clone()),
            }
        };
        let discard = {
            let store = self.store.clone();
            let this = cx.entity();
            let path = file.path.clone();
            let key = key.clone();
            let selector = format!("discard-{staged}-{}", file.path);
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .a11y_button(format!("Discard {}", file.path))
                .cursor_pointer()
                .rounded(px(4.))
                .h(px(24.))
                .min_w(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .hover(|s| s.bg(theme::tint(theme::DANGER)))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if armed {
                        this.update(cx, |this, _| this.discard_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(
                                GitAction::DiscardFile {
                                    path: path.clone(),
                                    is_staged: staged,
                                },
                                cx,
                            )
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.discard_armed = Some(key.clone());
                            cx.notify();
                        });
                    }
                })
                .when(armed, |d| {
                    d.px_1p5()
                        .text_size(px(theme::font_meta()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::DANGER)
                        .child("Confirm")
                })
                .when(!armed, |d| {
                    d.child(icon(
                        "icons/icon-undo.svg",
                        px(theme::icon_tiny()),
                        theme::text_dim(),
                    ))
                })
        };
        div()
            .id(row_id)
            .debug_selector(move || row_selector.to_string())
            .a11y_button(file.path.clone())
            .aria_selected(selected)
            .cursor_pointer()
            .group("file-row")
            .relative()
            .flex_shrink_0()
            // the list lays each item out as a root: without `w_full` a row
            // shrinks to its content and its hover/selected fill stops short
            // of the pane
            .w_full()
            .h(px(FILE_ROW_H))
            .pl(px(13. + depth as f32 * 14.))
            .pr_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_l_3()
            .border_color(if selected {
                theme::accent()
            } else {
                theme::surface()
            })
            .when(selected, |d| d.bg(theme::raised()))
            .when(!selected, |d| d.hover(|s| s.bg(theme::panel())))
            .on_click({
                let path = file.path.clone();
                cx.listener(move |this, _, window, cx| this.click_file(&path, staged, window, cx))
            })
            .child(
                div()
                    .w(px(14.))
                    .flex_shrink_0()
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(status_color)
                    .child(file.display_status_code().to_string()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .gap_1p5()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(theme::font_row_meta()))
                    .child(
                        div()
                            .when(self.tree_view, |d| {
                                d.min_w_0().overflow_hidden().text_ellipsis()
                            })
                            .when(!self.tree_view, |d| d.flex_shrink_0())
                            .text_color(theme::text())
                            .child(name),
                    )
                    .when(!dir.is_empty(), |row| {
                        row.child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_color(theme::text_muted())
                                .child(dir),
                        )
                    }),
            )
            .when(file.additions.is_some() && !armed, |row| {
                row.child(
                    div()
                        .id(SharedString::from(format!(
                            "counts-{}-{}",
                            if staged { "s" } else { "u" },
                            file.path
                        )))
                        .debug_selector({
                            let path = file.path.clone();
                            move || format!("counts-{}-{path}", if staged { "s" } else { "u" })
                        })
                        .flex_shrink_0()
                        .min_w(px(ROW_ACTIONS_W))
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_1p5()
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_micro()))
                        .group_hover("file-row", |s| s.invisible())
                        .when_some(file.additions, |row, added| {
                            row.child(
                                div()
                                    .text_color(theme::SUCCESS_BRIGHT)
                                    .child(format!("+{added}")),
                            )
                        })
                        .when_some(file.deletions, |row, deleted| {
                            row.child(div().text_color(theme::DANGER).child(format!("−{deleted}")))
                        }),
                )
            })
            // the hover verbs land on the counts' zone, which is kept at least
            // their width: the text stops before it, so an icon never sits on
            // the path, and no empty gutter is paid for when nothing hovers
            .child(
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(8.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_0p5()
                    .when(!armed, |d| {
                        d.invisible().group_hover("file-row", |s| s.visible())
                    })
                    .child(discard)
                    .child(row_action_button(file, staged, self.store.clone())),
            )
            .into_any_element()
    }

    /// The History tab: the commits on the left and the selected one on the
    /// right. The newest stands in until the user picks another.
    fn render_history(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let list = self.render_history_list(cx);
        // The commits own the row keys only while their handle holds the
        // focus; the column head says so with its ink and rule.
        let history_focused = self.history_focus.is_focused(window);
        if self.drilled_commit.is_none()
            && let Some(first) = self.history_rows.first().map(|row| row.hash.clone())
        {
            self.drill_commit(&first, cx);
        }
        let detail = match self.drilled_commit.clone() {
            Some(hash) => {
                let details = self.store.read(cx).commit_details.clone();
                self.render_commit_detail(hash, details, window, cx)
            }
            None => div().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .child(
                div()
                    .w(px(warpui_kit_core::shell::scaled(HISTORY_COLUMN_W)))
                    .flex_shrink_0()
                    .h_full()
                    .min_h_0()
                    .bg(theme::surface())
                    .border_r_1()
                    .border_color(theme::hairline())
                    // the commits take the keys only while this column owns
                    // the focus; the detail beside it is prose and controls
                    .track_focus(&self.history_focus)
                    .on_key_down(cx.listener(|this, event, window, cx| {
                        if this.handle_history_key(event, window, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            if !this.history_focus.is_focused(window) {
                                window.focus(&this.history_focus, cx);
                            }
                        }),
                    )
                    .child(Self::list_head(
                        "Commits",
                        self.history_rows.len(),
                        history_focused,
                        "history-head-focused",
                        "history-head-idle",
                        None,
                    ))
                    .child(list),
            )
            .child(
                div()
                    .debug_selector(|| "history-detail".into())
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .min_h_0()
                    .child(detail),
            )
            .into_any_element()
    }

    /// Day-grouped commits with unpushed markers. Row labels are precomputed
    /// (see [`HistoryRow`]); only the element tree is rebuilt per render.
    fn render_history_list(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (unpushed, generation) = {
            let store = self.store.read(cx);
            (store.unpushed.clone(), store.history_generation())
        };
        let today = chrono::Local::now().date_naive();
        if self.history_cache_key != (generation, today) {
            let yesterday = today - chrono::Duration::days(1);
            let mut current_day: Option<chrono::NaiveDate> = None;
            self.history_rows = self
                .store
                .read(cx)
                .history()
                .iter()
                .map(|commit| {
                    let day = commit.date.date_naive();
                    let day_label = if current_day != Some(day) {
                        current_day = Some(day);
                        Some(if day == today {
                            "Today".to_string()
                        } else if day == yesterday {
                            "Yesterday".to_string()
                        } else {
                            day.format("%b %e").to_string()
                        })
                    } else {
                        None
                    };
                    HistoryRow {
                        day_label,
                        initials: commit.initials(),
                        author: commit.author.clone().unwrap_or_else(|| "Unknown".into()),
                        rel: warp_station_core::rel_time::format(commit.date),
                        short_hash: commit.short_hash().to_string(),
                        subject: commit.subject.clone(),
                        hash: commit.hash.clone(),
                    }
                })
                .collect();
            self.history_items.clear();
            for (index, row) in self.history_rows.iter().enumerate() {
                if let Some(day) = &row.day_label {
                    self.history_items.push(HistoryItem::Day(day.clone()));
                }
                self.history_items.push(HistoryItem::Commit(index));
            }
            self.history_cache_key = (generation, today);
        }
        if self.history_rows.is_empty() {
            // An empty answer and a pending pass are different states: the
            // store settles the pass, so only a live one says "Loading".
            if !self.store.read(cx).history_loaded() {
                return div()
                    .id("history-list")
                    .debug_selector(|| "history-loading".into())
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text_dim())
                    .child(
                        div()
                            .id("history-loading-status")
                            .role(gpui::Role::Status)
                            .aria_label("Loading history")
                            .tooltip(crate::ui::tooltip_text("Loading history"))
                            .flex()
                            .items_center()
                            .child(motion::spinner(
                                "history-loading",
                                "icons/icon-spinner.svg",
                                px(theme::icon_tiny()),
                                theme::text_dim(),
                            )),
                    )
                    .child("Loading history…")
                    .into_any_element();
            }
            return div()
                .id("history-list")
                .debug_selector(|| "history-empty".into())
                .size_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_1p5()
                .p_6()
                .child(
                    div()
                        .text_size(px(theme::font_heading()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .text_align(gpui::TextAlign::Center)
                        .child("No commits yet"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_dim())
                        .text_align(gpui::TextAlign::Center)
                        .child("Commits on this branch will show here."),
                )
                .into_any_element();
        }

        // A day header and a commit have different heights, so the history is
        // a height-measured list, not a uniform one: a 26 px header row sits
        // between commit rows and never eats into the commit below it. It
        // still builds only the items in the viewport.
        let key = (generation, self.history_items.len());
        if self.history_list_key != key {
            self.history_list_key = key;
            self.history_list =
                ListState::new(self.history_items.len(), ListAlignment::Top, px(400.));
        }
        let items = Rc::new(self.history_items.clone());
        let rows = Rc::new(self.history_rows.clone());
        let unpushed = Rc::new(unpushed);
        let selected = self.drilled_commit.clone();
        let entity = cx.entity();
        let store = self.store.clone();
        let list = list(
            self.history_list.clone(),
            move |ix, _window, _cx| match &items[ix] {
                HistoryItem::Day(label) => history_day_row(label),
                HistoryItem::Commit(row_index) => {
                    let row = &rows[*row_index];
                    let selected = selected.as_deref() == Some(row.hash.as_str());
                    let unpushed = matches!(&*unpushed, Some(set) if set.contains(&row.hash));
                    history_row_element(row, unpushed, selected, entity.clone(), store.clone())
                }
            },
        );
        div()
            .id("history-list")
            .debug_selector(|| "history-list".into())
            .size_full()
            .min_h_0()
            .child(list.size_full())
            .into_any_element()
    }

    /// The drilled commit: the design's head band (subject, meta, actions)
    /// over its two-level split — the file rail and the selected file's diff.
    fn render_commit_detail(
        &mut self,
        hash: String,
        details: Option<std::sync::Arc<warp_station_core::model::GitCommitDetails>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(details) = details.filter(|d| d.hash.starts_with(&hash[..hash.len().min(7)]))
        else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::font_body()))
                .text_color(theme::text_dim())
                .child("Loading commit…")
                .into_any_element();
        };

        // author + time from the loaded history rows (the detail model
        // doesn't carry them)
        let history_meta = self
            .store
            .read(cx)
            .history()
            .iter()
            .find(|c| {
                c.hash == details.hash
                    || c.hash
                        .starts_with(&details.hash[..details.hash.len().min(7)])
            })
            .map(|c| {
                (
                    c.subject.clone(),
                    c.author.clone().unwrap_or_else(|| "Unknown".into()),
                    warp_station_core::rel_time::format(c.date),
                )
            });

        let additions = details
            .files
            .iter()
            .filter_map(|f| f.additions)
            .sum::<u32>();
        let deletions = details
            .files
            .iter()
            .filter_map(|f| f.deletions)
            .sum::<u32>();
        let file_count = details.files.len();
        let short_hash = details.hash[..7.min(details.hash.len())].to_string();

        // two-click armed destructive actions
        let armed = self.detail_armed;
        let action_button = |id: &'static str, label: &'static str, armed_now: bool| {
            let text = if armed_now { "Confirm" } else { label };
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .a11y_button(text)
                .cursor_pointer()
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .px_3()
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(if armed_now {
                    theme::DANGER
                } else {
                    theme::line_strong()
                })
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if armed_now {
                    theme::DANGER
                } else {
                    theme::text()
                })
                .hover(|s| s.bg(theme::raised()))
                .child(text.to_string())
        };

        // the design's commit-acts row: 8 gap, 10 under the meta line
        let mut actions_row = div()
            .mt(px(10.))
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .flex_shrink_0();
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            div()
                .id("detail-copy-sha")
                .debug_selector(|| "detail-copy-sha".into())
                .a11y_button("Copy SHA")
                .cursor_pointer()
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .px_3()
                .flex()
                .items_center()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::line_strong())
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::text())
                .hover(|s| s.bg(theme::raised()))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(full_hash.clone()));
                    store.update(cx, |s, cx| {
                        s.notify_success(format!(
                            "Copied {} to clipboard",
                            &full_hash[..7.min(full_hash.len())]
                        ));
                        cx.notify();
                    });
                })
                .child("Copy SHA")
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-revert", "Revert", armed == Some("revert")).on_click(
                move |_, _, cx| {
                    if armed == Some("revert") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(GitAction::Revert(full_hash.clone()), cx)
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("revert");
                            cx.notify();
                        });
                    }
                },
            )
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-checkout", "Checkout", armed == Some("checkout")).on_click(
                move |_, _, cx| {
                    if armed == Some("checkout") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(GitAction::Checkout(full_hash.clone()), cx)
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("checkout");
                            cx.notify();
                        });
                    }
                },
            )
        });

        // the head band: subject, then author · age · SHA · N files · +a −d,
        // then the verbs, on the panel surface
        let (author, rel) = history_meta
            .as_ref()
            .map(|(_, author, rel)| (author.clone(), rel.clone()))
            .unwrap_or_else(|| ("Unknown".into(), String::new()));
        let separator = || div().text_color(theme::text_faint()).child("·");
        let meta = div()
            .mt_0p5()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_1p5()
            .text_size(px(theme::font_meta()))
            .text_color(theme::text_dim())
            .child(format!("{author} · {rel}"))
            .child(separator())
            .child(
                div()
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .text_color(theme::text_faint())
                    .child(short_hash.clone()),
            )
            .child(separator())
            .child(format!(
                "{file_count} {}",
                if file_count == 1 { "file" } else { "files" }
            ))
            .child(separator())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .child(
                        div()
                            .text_color(theme::SUCCESS_BRIGHT)
                            .child(format!("+{additions}")),
                    )
                    .child(
                        div()
                            .text_color(theme::DANGER)
                            .child(format!("−{deletions}")),
                    ),
            );
        let head = div()
            .flex_shrink_0()
            .px_4()
            .py(px(14.))
            .bg(theme::panel())
            .border_b_1()
            .border_color(theme::hairline())
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "commit-subject".into())
                    .text_size(px(theme::font_heading()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child(
                        history_meta
                            .as_ref()
                            .map(|(subject, ..)| subject.clone())
                            .unwrap_or_else(|| short_hash.clone()),
                    ),
            )
            .child(meta)
            .child(actions_row);

        // the files rail: a 240 column's group header over one 34-high row
        // per changed file, selected file on the raised surface
        let selected_file = details
            .files
            .len()
            .checked_sub(1)
            .map(|last| self.commit_file.min(last))
            .unwrap_or(0);
        // the rail is virtualized too: a ten-thousand-file commit builds only
        // the rows on screen, and the details are an Arc so this pane does not
        // clone the whole file list on every repaint. The tree's folders fold;
        // the selection stays the file whose diff is shown.
        self.pane_nav.register(&self.commit_rail_focus);
        let rail_items = self.commit_rail_items(&details);
        let rail_files = details.clone();
        let rows = uniform_list(
            SharedString::from("commit-files"),
            rail_items.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                range
                    .map(|ix| match rail_items[ix].clone() {
                        RailItem::Folder(folder) => this.render_commit_folder_row(&folder, cx),
                        RailItem::File { index, depth } => this.render_commit_file_row(
                            &rail_files,
                            index,
                            selected_file,
                            depth,
                            cx,
                        ),
                    })
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .py_1()
        .track_scroll(&self.commit_rail_scroll)
        .debug_selector(|| "commit-files".into())
        .into_any_element();
        let rail = div()
            .w(px(warpui_kit_core::shell::scaled(COMMIT_RAIL_W)))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(theme::surface())
            .border_r_1()
            .border_color(theme::hairline())
            .track_focus(&self.commit_rail_focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                if this.handle_commit_rail_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.commit_rail_focus.is_focused(window) {
                        window.focus(&this.commit_rail_focus, cx);
                    }
                }),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(COMMIT_RAIL_ROW_H))
                    .pl_4()
                    .pr_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_meta()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text_dim())
                    .child("Files")
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme::text_muted())
                            .child(file_count.to_string()),
                    ),
            )
            .child(rows);

        // the selected file's diff, through the Changes tab's own pane
        let diff = match details.files.get(selected_file) {
            Some(file) => {
                let key = format!("{}|{}", details.hash, file.path);
                let patch = self.detail_patches.get(&key).map(String::as_str);
                self.render_diff_pane(
                    "commit-diff",
                    "commit-diff-menu",
                    &file.path,
                    patch,
                    window,
                    cx,
                )
            }
            None => div()
                .id("commit-diff")
                .debug_selector(|| "commit-diff".into())
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(theme::well())
                .child(diff_controls::state("No files in this commit"))
                .into_any_element(),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(head)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .child(rail)
                    .child(diff),
            )
            .into_any_element()
    }

    /// One fixed-height row of the virtualized commit rail: the status
    /// letter, the dir/name split and the line counts, on the raised surface
    /// when it is the selected file.
    /// A folder row of the commit rail: the Changes list's band without the
    /// verbs — this rail is read-only.
    fn render_commit_folder_row(
        &mut self,
        folder: &FolderItem,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let folded = self.commit_folded.contains(folder.path.as_ref());
        let cursor = self.commit_cursor.as_ref()
            == Some(&RailKey {
                path: folder.path.to_string(),
                folder: true,
            });
        let selector = format!("commit-folder-{}", folder.path);
        let row_selector = selector.clone();
        let label: SharedString = format!(
            "{} ({} {})",
            folder.label,
            folder.count,
            if folder.count == 1 { "file" } else { "files" }
        )
        .into();
        let click_folder = folder.clone();
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || row_selector.clone())
            .a11y_tree_item(label, Some(!folded))
            .cursor_pointer()
            .w_full()
            .h(px(COMMIT_RAIL_ROW_H))
            .pl(px(13. + folder.depth as f32 * 14.))
            .pr_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .when(cursor, |d| d.bg(theme::raised()))
            .hover(|s| s.bg(theme::panel()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.commit_cursor = Some(RailKey {
                    path: click_folder.path.to_string(),
                    folder: true,
                });
                this.toggle_commit_folder(&click_folder.path, cx);
                window.focus(&this.commit_rail_focus, cx);
            }))
            .child(icon(
                if folded {
                    "icons/icon-chevron-right.svg"
                } else {
                    "icons/icon-chevron-down.svg"
                },
                px(theme::icon_tiny()),
                theme::text_faint(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text())
                    .child(folder.label.to_string()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::text_muted())
                    .child(folder.count.to_string()),
            )
            .into_any_element()
    }

    fn render_commit_file_row(
        &mut self,
        details: &std::sync::Arc<warp_station_core::model::GitCommitDetails>,
        index: usize,
        selected_file: usize,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let file = &details.files[index];
        let selected = index == selected_file;
        let (dir, name) = if self.tree_view {
            let name = file
                .path
                .rsplit('/')
                .next()
                .unwrap_or(file.path.as_str())
                .to_string();
            (String::new(), name)
        } else {
            match file.path.rsplit_once('/') {
                Some((dir, name)) => (format!("{dir}/"), name.to_string()),
                None => (String::new(), file.path.clone()),
            }
        };
        let status = file.display_status_code().to_string();
        let mut counts = div()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .font_family(gpui::SharedString::from(theme::code_font()))
            .text_size(px(theme::font_micro()));
        if let Some(added) = file.additions.filter(|added| *added > 0) {
            counts = counts.child(
                div()
                    .text_color(theme::SUCCESS_BRIGHT)
                    .child(format!("+{added}")),
            );
        }
        if let Some(deleted) = file.deletions.filter(|deleted| *deleted > 0) {
            counts = counts.child(div().text_color(theme::DANGER).child(format!("−{deleted}")));
        }
        div()
            .id(SharedString::from(format!("commit-file-{index}")))
            .debug_selector(move || format!("commit-file-{index}"))
            .a11y_button(file.path.clone())
            .aria_selected(selected)
            .cursor_pointer()
            .flex_shrink_0()
            .w_full()
            .h(px(COMMIT_RAIL_ROW_H))
            .pl(px(13. + depth as f32 * 14.))
            .pr_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_l_3()
            .border_color(if selected {
                theme::accent()
            } else {
                theme::surface()
            })
            .when(selected, |d| d.bg(theme::raised()))
            .when(!selected, |d| d.hover(|s| s.bg(theme::panel())))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_commit_file(index, cx);
                window.focus(&this.commit_rail_focus, cx);
            }))
            .child(
                div()
                    .w(px(14.))
                    .flex_shrink_0()
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .text_size(px(theme::font_mono_meta()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(diff_controls::status_color(&status))
                    .child(status),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .gap_1p5()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(theme::font_meta()))
                    .child(
                        div()
                            .when(self.tree_view, |d| {
                                d.min_w_0().overflow_hidden().text_ellipsis()
                            })
                            .when(!self.tree_view, |d| d.flex_shrink_0())
                            .text_color(theme::text())
                            .child(name),
                    )
                    .when(!dir.is_empty(), |line| {
                        line.child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .text_color(theme::text_muted())
                                .child(dir),
                        )
                    }),
            )
            .child(counts)
            .into_any_element()
    }

    /// The middle card: "Commit message" title, the message textarea with
    /// the wand in its corner, and the outlined commit button. An empty box
    /// turns the button into "Generate & Commit": the wand drafts the
    /// message first, then the commit fires itself.
    fn render_commit_card(
        &mut self,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (wand_active, wand_queued_here, wand_live) = {
            let state = self.store.read(cx);
            let here = state.selected_repo.as_deref();
            let active = state.wand_busy == Some(crate::net::WandKind::Commit)
                && state.wand_repo.as_deref() == here;
            let queued =
                here.is_some_and(|folder| state.wand_queued(crate::net::WandKind::Commit, folder));
            (active, queued, state.wand_busy.is_some())
        };
        let wand_on = self.store.read(cx).wand_available();
        let message_empty = self.commit_message.read(cx).value().trim().is_empty();
        // one run owns the slot at a time; a request made while it is busy
        // queues for this repo and starts when the live run ends
        let can_generate = wand_on && !wand_active && !wand_queued_here;
        let generate_now = message_empty && can_generate && !wand_live;
        let queue_generation = message_empty && can_generate && wand_live;
        let generate_and_commit = generate_now;

        let mut card = div()
            .w_full()
            .flex_shrink_0()
            .h(px(COMMIT_BOX_H))
            .p_3()
            .border_t_1()
            .border_color(theme::hairline())
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .on_key_down({
                        let store = self.store.clone();
                        let message = self.commit_message.clone();
                        move |event, _, cx| {
                            if event.keystroke.key == "enter"
                                && event.keystroke.modifiers.control
                                && !message.read(cx).value().trim().is_empty()
                            {
                                let text = message.read(cx).value().to_string();
                                store.update(cx, |s, cx| {
                                    s.run_git(GitAction::Commit(text.clone()), cx)
                                });
                            }
                        }
                    })
                    .child(
                        // the design's `textarea.area`: the recessed well with
                        // the default line, the body step on 9/10 padding
                        warpui_kit_core::field::area(
                            "commit-message",
                            &self.commit_message,
                            window,
                            cx,
                        )
                        .h_full()
                        // the well is one flex slot, not a content-sized box:
                        // without the zero floor a long draft grows the box
                        // past the card's row and paints under the button;
                        // the editor scrolls inside the fixed well instead
                        .min_h(px(0.))
                        // the card carries one accent — the commit button
                        // under the box — so the box keeps its resting edge
                        // while the draft has focus instead of turning accent
                        // and reading as a second, competing action
                        .focus(|s| s.border_color(theme::hairline())),
                    )
                    // the corner wand: overlay inside the message box
                    .when(can_generate, |box_area| {
                        let store = self.store.clone();
                        box_area.child(
                            div()
                                .id("wand-corner")
                                .a11y_button("Generate commit message")
                                .absolute()
                                .bottom_1()
                                .right_2()
                                .cursor_pointer()
                                .rounded_md()
                                .p_1()
                                .flex()
                                .hover(|s| s.bg(theme::hover_bg()))
                                .on_click(move |_, _, cx| {
                                    store.update(cx, |s, cx| {
                                        s.run_wand(crate::net::WandKind::Commit, false, cx)
                                    });
                                })
                                .child(icon(
                                    "icons/icon-wand.svg",
                                    px(theme::icon_small()),
                                    theme::text_dim(),
                                )),
                        )
                    }),
            );

        let mut button_row = div().flex().flex_row().items_center().gap_2();
        button_row = button_row.child({
            let store = self.store.clone();
            let message = self.commit_message.clone();
            let (generating, committing) = (
                wand_active,
                self.store.read(cx).running_job() == Some("commit"),
            );
            let working = generating || committing;
            let label = if generating {
                "Generating…"
            } else if committing {
                "Committing…"
            } else if message_empty && wand_queued_here {
                "Queued…"
            } else if generate_and_commit {
                "Generate & Commit"
            } else if queue_generation {
                "Queue & Commit"
            } else {
                "Commit"
            };
            let can_commit = !busy && (!message_empty || can_generate);
            let color = if can_commit {
                theme::on_accent()
            } else {
                theme::text_muted()
            };
            div()
                .id("commit-button")
                .debug_selector(|| "commit-button".into())
                .a11y_button(label)
                .tooltip(crate::ui::tooltip_text(label))
                .cursor_pointer()
                .flex_1()
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .gap_1p5()
                .text_color(color)
                .when(can_commit, |b| {
                    b.bg(theme::accent()).hover(|s| s.bg(theme::accent_hover()))
                })
                .when(!can_commit, |b| b.bg(theme::raised()))
                .on_click(move |_, _, cx| {
                    if busy {
                        return;
                    }
                    // the box only exists with staged files, but the state may
                    // have changed since this frame was built
                    let staged = store
                        .read(cx)
                        .changes()
                        .is_some_and(|snapshot| !snapshot.staged_files.is_empty());
                    if !staged {
                        return;
                    }
                    let text = message.read(cx).value().to_string();
                    if text.trim().is_empty() {
                        // no wand, or this repo's request is already live or
                        // queued: there is nothing this click could start
                        if !can_generate {
                            return;
                        }
                        // empty box: generate, then commit in this repo once
                        // the run lands — even if another repo is on screen
                        // by then; the store owns that follow-up
                        store.update(cx, |s, cx| {
                            s.run_wand(crate::net::WandKind::Commit, true, cx)
                        });
                        return;
                    }
                    store.update(cx, |s, cx| s.run_git(GitAction::Commit(text.clone()), cx));
                })
                .when(working, |b| {
                    b.child(warpui_kit_core::motion::spinner(
                        "commit-spinner",
                        "icons/icon-spinner.svg",
                        px(theme::icon_small()),
                        color,
                    ))
                })
                .when(!working, |b| {
                    b.child(icon(
                        if generate_and_commit
                            || queue_generation
                            || (message_empty && wand_queued_here)
                        {
                            "icons/icon-wand.svg"
                        } else {
                            "icons/icon-check.svg"
                        },
                        px(theme::icon_small()),
                        color,
                    ))
                })
                .child(
                    div()
                        .text_size(px(theme::font_control()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color)
                        .child(label),
                )
        });
        if wand_active {
            let store = self.store.clone();
            button_row = button_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::blue_bright())
                    .child(icon(
                        "icons/icon-wand.svg",
                        px(theme::icon_small() - 1.),
                        theme::blue_bright(),
                    ))
                    .child("working…")
                    // the one run can hang on a dead harness; never leave the
                    // box without a way out
                    .child(
                        div()
                            .id("wand-cancel")
                            .debug_selector(|| "wand-cancel".into())
                            .a11y_button("Cancel generation")
                            .cursor_pointer()
                            .ml_2()
                            .px_1p5()
                            .rounded(px(4.))
                            .text_color(theme::text_dim())
                            .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text()))
                            .on_click(move |_, _, cx| store.update(cx, |s, cx| s.cancel_wand(cx)))
                            .child("Cancel"),
                    ),
            );
        }
        if wand_queued_here {
            let store = self.store.clone();
            let folder = self
                .store
                .read(cx)
                .selected_repo
                .clone()
                .unwrap_or_default();
            button_row = button_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::font_body()))
                    .text_color(theme::text_dim())
                    .child(icon(
                        "icons/icon-wand.svg",
                        px(theme::icon_small() - 1.),
                        theme::text_dim(),
                    ))
                    .child("queued…")
                    // the request waits for the slot: pulling it back must
                    // not stop the run that is live now
                    .child(
                        div()
                            .id("wand-dequeue")
                            .debug_selector(|| "wand-dequeue".into())
                            .a11y_button("Cancel queued generation")
                            .cursor_pointer()
                            .ml_2()
                            .px_1p5()
                            .rounded(px(4.))
                            .text_color(theme::text_dim())
                            .hover(|s| s.bg(theme::hover_bg()).text_color(theme::text()))
                            .on_click(move |_, _, cx| {
                                store.update(cx, |s, cx| {
                                    s.dequeue_wand(crate::net::WandKind::Commit, &folder, cx)
                                })
                            })
                            .child("Cancel"),
                    ),
            );
        }
        card = card.child(button_row);
        card.into_any_element()
    }

    /// The git toolbar over the columns: the branch selector, then the sync
    /// verbs. Pull and Push carry what they would move; a branch with no
    /// upstream publishes instead of pushing.
    fn render_toolbar(&mut self, busy: bool, cx: &mut Context<Self>) -> gpui::AnyElement {
        let store = self.store.read(cx);
        let entry = store.selected_entry();
        let branch = entry.and_then(|e| e.git_branch_name.clone());
        let has_remote = entry.is_some_and(|e| e.git_has_remote);
        let published = entry.is_some_and(|e| e.git_branch_published);
        let to_push = entry.map_or(0, |e| e.git_to_push_count);
        let to_pull = entry.map_or(0, |e| e.git_to_pull_count);
        let job = store.running_job();
        let finished = store.finished_job();

        let action = |icon_path: &'static str,
                      label: &'static str,
                      enabled: bool,
                      badge: Option<(u32, gpui::Rgba)>,
                      git: GitAction| {
            let store = self.store.clone();
            let running = job == Some(git.label());
            let landed = finished == Some(git.label());
            let word = if running {
                crate::net::git_working_word(git.label())
            } else if landed {
                crate::net::git_done_word(git.label())
            } else {
                label
            };
            div()
                .id(SharedString::from(format!("rail-{label}")))
                .debug_selector(move || format!("rail-{label}"))
                .a11y_button(word)
                .tooltip(crate::ui::tooltip_text(word))
                .flex_shrink_0()
                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                .px_3()
                .rounded(px(6.))
                .border_1()
                .border_color(theme::line_strong())
                .bg(theme::panel())
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_control()))
                .font_weight(FontWeight::MEDIUM)
                .when(enabled, |d| {
                    d.cursor_pointer()
                        .text_color(theme::text())
                        .hover(|s| s.bg(theme::raised()))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            store.update(cx, |s, cx| s.run_git(git.clone(), cx));
                        })
                })
                // the running button stays at full strength
                .when(!enabled && !running, |d| {
                    d.opacity(0.45).text_color(theme::text_dim())
                })
                .when(running, |d| d.text_color(theme::text()))
                .when(landed, |d| d.text_color(theme::SUCCESS_BRIGHT))
                .child(if running {
                    warpui_kit_core::motion::spinner(
                        SharedString::from(format!("rail-spinner-{label}")),
                        "icons/icon-spinner.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )
                } else if landed {
                    icon(
                        "icons/icon-check.svg",
                        px(theme::icon_small()),
                        theme::SUCCESS_BRIGHT,
                    )
                    .into_any_element()
                } else {
                    icon(icon_path, px(theme::icon_small()), theme::text_dim()).into_any_element()
                })
                .child(word)
                .when_some(badge.filter(|_| enabled), |row, (count, color)| {
                    let selector = format!("rail-badge-{label}");
                    row.child(
                        div()
                            .debug_selector(move || selector.clone())
                            .text_color(color)
                            .child(format!("{count}")),
                    )
                })
        };

        // Only the verbs that can do something right now: a pull with nothing
        // behind, a push with nothing ahead, or a fetch with no remote is not
        // offered at all. The running/just-landed button stays visible so its
        // progress and past-tense confirmation are not yanked away.
        let on_branch = branch.as_deref().is_some_and(|name| name != "(detached)");
        let visible =
            |label: &str, relevant: bool| relevant || job == Some(label) || finished == Some(label);
        let mut bar = div()
            .flex_shrink_0()
            .h(px(TOOLBAR_H))
            .px_2()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(theme::hairline())
            .child(div().w(px(BRANCH_W)).child(self.branch_picker.clone()));
        if visible("fetch", has_remote) {
            bar = bar.child(action(
                "icons/icon-cloud-download.svg",
                "Fetch",
                !busy,
                None,
                GitAction::Fetch,
            ));
        }
        if visible("pull", has_remote && on_branch && to_pull > 0) {
            bar = bar.child(action(
                "icons/icon-arrow-down.svg",
                "Pull",
                !busy,
                (to_pull > 0).then_some((to_pull, theme::BEHIND)),
                GitAction::Pull,
            ));
        }
        match branch.clone() {
            // a branch the remote does not have yet: publishing is the move
            Some(name)
                if has_remote
                    && on_branch
                    && (!published || job == Some("publish") || finished == Some("publish")) =>
            {
                bar = bar.child(action(
                    "icons/icon-arrow-up.svg",
                    "Publish",
                    !busy,
                    None,
                    GitAction::Publish { branch: name },
                ));
            }
            Some(_)
                if has_remote
                    && on_branch
                    && (to_push > 0 || job == Some("push") || finished == Some("push")) =>
            {
                bar = bar.child(action(
                    "icons/icon-arrow-up.svg",
                    "Push",
                    !busy,
                    (to_push > 0).then_some((to_push, theme::AHEAD)),
                    GitAction::Push,
                ));
            }
            _ => {}
        }
        if visible(
            "sync",
            has_remote && on_branch && (to_pull > 0 || to_push > 0),
        ) {
            bar = bar.child(action(
                "icons/icon-refresh.svg",
                "Sync",
                !busy,
                None,
                GitAction::Sync,
            ));
        }
        // the pushed branch's next step: the form, or the pull request that
        // already has this head. It is state, so it survives a restart and a
        // repo switch; the Workflow setting only decides whether it also opens
        // by itself.
        if let Some(shortcut) = store.pull_shortcut() {
            // the toolbar is tight on a stacked pane: the verb keeps its icon
            // and shortens its word, the full sentence stays the tooltip
            let compact = store.pane_width < 900.;
            let show_tooltips = store.settings().general.show_tooltips;
            bar = bar.child(div().flex_1()).child(pull_shortcut_button(
                shortcut,
                self.store.clone(),
                compact,
                show_tooltips,
            ));
        }
        bar.into_any_element()
    }

    /// The branch row's context menu: check the row out, copy its name, delete
    /// it. It opens at the pointer, closes on any press outside, on Escape, and
    /// after an action.
    fn render_branch_menu(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let menu = self.branch_ui.borrow().menu.clone()?;
        let (current, busy) = {
            let store = self.store.read(cx);
            (
                store
                    .selected_entry()
                    .and_then(|entry| entry.git_branch_name.clone())
                    .unwrap_or_default(),
                store.git_busy(),
            )
        };
        let branch = menu.branch.clone();
        let is_current = !current.is_empty() && branch == current;
        let this = cx.entity();
        let close = {
            let this = this.clone();
            move |cx: &mut gpui::App| {
                this.update(cx, |view, cx| view.close_branch_menu(cx));
            }
        };
        let mut card = crate::ui::pull_request::menu_shell("branch-row-menu");
        card = card.child(
            div()
                .px_2()
                .py_1()
                .truncate()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(branch.clone()),
        );

        let store = self.store.clone();
        let checkout_name = branch.clone();
        let checkout_close = close.clone();
        card = card.child(crate::ui::pull_request::menu_item(
            "branch-menu-checkout",
            "icons/icon-circle-dot.svg",
            "Checkout",
            !is_current && !busy,
            move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.run_git(GitAction::Checkout(checkout_name.clone()), cx);
                });
                checkout_close(cx);
            },
        ));

        // merging is the branch row's second verb: the selected branch into
        // the checked-out one, which stops on conflicts and opens the editor
        let merge_name = branch.clone();
        let merge_close = close.clone();
        let merge_store = self.store.clone();
        card = card.child(crate::ui::pull_request::menu_item(
            "branch-menu-merge",
            "icons/icon-merge.svg",
            "Merge into current",
            !is_current && !busy,
            move |_, _, cx| {
                merge_store.update(cx, |store, cx| {
                    store.run_git(GitAction::Merge(merge_name.clone()), cx);
                });
                merge_close(cx);
            },
        ));

        let copy_name = branch.clone();
        let copy_close = close.clone();
        card = card.child(crate::ui::pull_request::menu_item(
            "branch-menu-copy",
            "icons/icon-copy.svg",
            "Copy branch name",
            true,
            move |_, _, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy_name.clone()));
                copy_close(cx);
            },
        ));

        // Delete: a local, non-current branch, armed by the first press.
        let delete_name = branch.clone();
        let delete_close = close.clone();
        let delete_armed = menu.delete_armed;
        let delete_store = self.store.clone();
        let delete_this = this.clone();
        card = card.child(crate::ui::pull_request::menu_divider()).child(
            crate::ui::pull_request::menu_item(
                "branch-menu-delete",
                "icons/icon-trash.svg",
                if delete_armed {
                    "Confirm delete"
                } else {
                    "Delete branch"
                },
                !menu.is_remote && !is_current && !busy,
                move |_, _, cx| {
                    if delete_armed {
                        delete_store.update(cx, |store, cx| {
                            store.run_git(GitAction::DeleteBranch(delete_name.clone()), cx);
                        });
                        delete_close(cx);
                    } else {
                        // first press arms: the row turns into "Confirm
                        // delete" in place, the menu stays up
                        delete_this.update(cx, |view, cx| {
                            if let Some(menu) = view.branch_ui.borrow_mut().menu.as_mut() {
                                menu.delete_armed = true;
                            }
                            cx.notify();
                        });
                    }
                },
            ),
        );

        let catcher = div()
            .id("branch-menu-catcher")
            .debug_selector(|| "branch-menu-catcher".into())
            .absolute()
            .inset_0()
            .occlude()
            .track_focus(&self.menu_focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.close_branch_menu(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_branch_menu(cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.close_branch_menu(cx)),
            )
            .child(
                gpui::anchored()
                    .anchor(gpui::Anchor::TopLeft)
                    .position(menu.at)
                    .offset(gpui::point(px(6.), px(6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(card),
            );
        // deferred with the picker's popup, and after it in tree order: the
        // row menu paints over a dropdown that stayed open under it
        Some(gpui::deferred(catcher).with_priority(1).into_any_element())
    }

    fn close_branch_menu(&mut self, cx: &mut Context<Self>) {
        if self.branch_ui.borrow_mut().menu.take().is_some() {
            // the picker stayed up under the menu; closing the menu closes
            // the dropdown with it
            self.branch_picker.update(cx, FilteredPicker::dismiss);
            cx.notify();
        }
    }
}

/// A day header's own measured row: its own 26 px, so the commit below keeps
/// its full height.
fn history_day_row(label: &str) -> gpui::AnyElement {
    div()
        .id(SharedString::from(format!("history-day-{label}")))
        .debug_selector(move || format!("history-day-{label}"))
        .w_full()
        .h(px(HISTORY_DAY_HEIGHT))
        .flex_shrink_0()
        .px_4()
        .flex()
        .items_end()
        .pb_0p5()
        .text_size(px(theme::font_sub()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text_dim())
        .child(label.to_string())
        .into_any_element()
}

/// One commit row: 58 px, the subject over the author/meta line. The day
/// header is a separate item, so this row is never squeezed.
fn history_row_element(
    row: &HistoryRow,
    unpushed_mark: bool,
    selected: bool,
    entity: Entity<ChangesView>,
    store: Entity<Store>,
) -> gpui::AnyElement {
    div()
        .id(SharedString::from(format!("commit-{}", row.short_hash)))
        .debug_selector({
            let short = row.short_hash.clone();
            move || format!("commit-{short}")
        })
        .a11y_button(format!("{} — {}, {}", row.subject, row.author, row.rel))
        .cursor_pointer()
        .flex_shrink_0()
        .w_full()
        .h(px(HISTORY_ROW_HEIGHT))
        .flex()
        .flex_col()
        .justify_center()
        .child(
            div()
                .id(SharedString::from(format!(
                    "commit-body-{}",
                    row.short_hash
                )))
                .debug_selector({
                    let short = row.short_hash.clone();
                    move || format!("commit-body-{short}")
                })
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .justify_center()
                .gap_0p5()
                .pl(px(13.))
                .pr_3()
                .border_l_3()
                .border_color(if selected {
                    theme::accent()
                } else {
                    theme::surface()
                })
                .when(selected, |d| d.bg(theme::raised()))
                .when(!selected, |d| d.hover(|s| s.bg(theme::panel())))
                .on_click({
                    let hash = row.hash.clone();
                    move |_, _, cx| {
                        entity.update(cx, |this, cx| this.drill_commit(&hash, cx));
                    }
                })
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(row.subject.clone()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .min_w_0()
                        .overflow_hidden()
                        .child(
                            div()
                                .rounded_full()
                                .bg(theme::chip())
                                .w(px(20.))
                                .h(px(20.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(px(theme::font_micro()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::text_muted())
                                .child(row.initials.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(theme::font_meta()))
                                .text_color(theme::text_dim())
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(format!("{} · {}", row.author, row.rel)),
                        )
                        .when(unpushed_mark, |meta| {
                            meta.child(
                                div()
                                    .id(SharedString::from(format!("unpushed-{}", row.short_hash)))
                                    .role(gpui::Role::Status)
                                    .aria_label("unpushed")
                                    .flex_shrink_0()
                                    .text_size(px(theme::font_meta()))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme::AHEAD)
                                    .tooltip(crate::ui::tooltip_text("Not pushed yet"))
                                    .child("\u{2191}"),
                            )
                        })
                        .child(
                            div()
                                .id(SharedString::from(format!("copy-sha-{}", row.short_hash)))
                                .a11y_button(format!("Copy SHA {}", row.short_hash))
                                .cursor_pointer()
                                .flex_shrink_0()
                                .rounded_md()
                                .px_1()
                                .text_size(px(theme::font_meta()))
                                .font_family(gpui::SharedString::from(theme::code_font()))
                                .text_color(theme::text_faint())
                                .hover(|s| s.text_color(theme::blue_bright()))
                                .on_click({
                                    let full = row.hash.clone();
                                    let short = row.short_hash.clone();
                                    move |_, _, cx| {
                                        cx.stop_propagation();
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            full.clone(),
                                        ));
                                        store.update(cx, |s, cx| {
                                            s.notify_success(format!(
                                                "Copied {short} to clipboard"
                                            ));
                                            cx.notify();
                                        });
                                    }
                                })
                                .child(row.short_hash.clone()),
                        ),
                ),
        )
        .into_any_element()
}

/// The git toolbar's PR verb. Create opens the writing form (GitHub) or the
/// platform's create page (Azure); View opens the pull request the branch
/// already has.
fn pull_shortcut_button(
    shortcut: PullShortcut,
    store: Entity<Store>,
    compact: bool,
    show_tooltips: bool,
) -> gpui::AnyElement {
    use warp_station_core::activity::ActivitySource;
    let reference = match shortcut {
        PullShortcut::Create { .. } => None,
        PullShortcut::Open { source, number } => Some(if source == ActivitySource::Azure {
            format!("!{number}")
        } else {
            format!("#{number}")
        }),
    };
    let label = reference.as_deref().map_or_else(
        || {
            if compact {
                "PR".to_string()
            } else {
                "Create PR".to_string()
            }
        },
        |reference| {
            if compact {
                reference.to_string()
            } else {
                format!("View PR {reference}")
            }
        },
    );
    let azure_create = matches!(
        shortcut,
        PullShortcut::Create {
            source: ActivitySource::Azure
        }
    );
    let a11y = reference.as_deref().map_or_else(
        || {
            if azure_create {
                "Create pull request on Azure DevOps".to_string()
            } else {
                "Create pull request".to_string()
            }
        },
        |reference| format!("View pull request {reference}"),
    );
    let tooltip = a11y.clone();
    let selector = if reference.is_some() {
        "rail-view-pr"
    } else {
        "rail-create-pr"
    };
    let button = div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .a11y_button(a11y)
        .flex_shrink_0()
        .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .px_3()
        .rounded(px(6.))
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::panel())
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::text())
        .hover(|style| style.bg(theme::raised()))
        .on_click(move |_, _, cx| {
            store.update(cx, |store, cx| match shortcut {
                PullShortcut::Create { .. } => store.start_new_pull(cx),
                PullShortcut::Open { source, number } => {
                    store.open_pull_shortcut(source, number, cx)
                }
            });
        })
        .child(icon(
            "icons/icon-git-pull-request.svg",
            px(theme::icon_small()),
            theme::text_dim(),
        ))
        .child(label);
    crate::ui::with_tooltip(button, show_tooltips, tooltip).into_any_element()
}

fn section_header(
    section: FileSection,
    count: usize,
    collapsed: bool,
    this: Entity<ChangesView>,
    trailing: Vec<gpui::AnyElement>,
) -> impl IntoElement + use<> {
    let label = section.label();
    let selector = section.selector();
    let title = div()
        .id(SharedString::from(selector))
        .debug_selector(move || selector.to_string())
        .a11y_button(format!("{label} ({count})"))
        .aria_expanded(!collapsed)
        .cursor_pointer()
        .min_w_0()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .on_click(move |_, _, cx| {
            this.update(cx, |view, cx| view.toggle_section(section, cx));
        })
        .child(icon(
            if collapsed {
                "icons/icon-chevron-right.svg"
            } else {
                "icons/icon-chevron-down.svg"
            },
            px(theme::icon_tiny()),
            theme::text_muted(),
        ))
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(theme::font_body()))
                .text_color(theme::text_faint())
                .child(format!("({count})")),
        );

    // the header's verbs are hover-revealed like the rows' actions: the
    // container keeps their width, so the title does not jump when they show
    let mut actions = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .flex_shrink_0()
        .invisible()
        .group_hover(selector, |s| s.visible());
    for element in trailing {
        actions = actions.child(element);
    }

    div()
        .group(selector)
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .w_full()
        .h(px(FILE_ROW_H))
        .px_1()
        .rounded(px(4.))
        .hover(|s| s.bg(theme::panel()))
        .child(title.flex_1())
        .child(actions)
}

/// The unstaged header's discard-all: the row discard's arrow glyph, so the
/// whole-tree verb reads as the same action its rows offer. Its first click
/// arms it (the glyph becomes "Confirm") and its second throws every unstaged
/// change away — the row discards take their confirmation the same way.
fn discard_all_button(
    armed: bool,
    files: Rc<Vec<String>>,
    store: Entity<Store>,
    this: Entity<ChangesView>,
    show_tooltips: bool,
) -> gpui::AnyElement {
    let label = if armed {
        "Confirm discard"
    } else {
        "Discard all unstaged changes"
    };
    let button = div()
        .id("discard-all-u")
        .debug_selector(|| "discard-all-u".into())
        .a11y_button(label)
        .cursor_pointer()
        .rounded_md()
        .p_1()
        .flex()
        .items_center()
        .hover(|s| s.bg(theme::tint(theme::DANGER)))
        .on_click(move |_, _, cx| {
            if armed {
                this.update(cx, |this, _| this.discard_all_armed = false);
                store.update(cx, |s, cx| {
                    s.run_git(GitAction::DiscardUnstaged((*files).clone()), cx)
                });
            } else {
                this.update(cx, |this, cx| {
                    this.discard_all_armed = true;
                    cx.notify();
                });
            }
        })
        .when(armed, |d| {
            d.px_1p5()
                .text_size(px(theme::font_meta()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::DANGER)
                .child("Confirm")
        })
        .when(!armed, |d| {
            d.child(icon(
                "icons/icon-undo.svg",
                px(theme::icon_small()),
                theme::text_muted(),
            ))
        });
    crate::ui::with_tooltip(button, show_tooltips, label).into_any_element()
}

/// A whole-list staging verb as an icon button — the compact form both
/// section headers use, with the verb in the tooltip and the a11y name. The
/// stage footer keeps the primary button.
fn staging_icon(
    id: &'static str,
    icon_path: &'static str,
    label: &'static str,
    action: GitAction,
    store: Entity<Store>,
    show_tooltips: bool,
) -> gpui::AnyElement {
    let selector = id.to_string();
    let button = div()
        .id(SharedString::from(id))
        .debug_selector(move || selector.clone())
        .a11y_button(label)
        .cursor_pointer()
        .rounded_md()
        .p_1()
        .flex()
        .items_center()
        .hover(|s| s.bg(theme::raised()))
        .on_click(move |_, _, cx| {
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(icon(
            icon_path,
            px(theme::icon_small()),
            theme::text_muted(),
        ));
    crate::ui::with_tooltip(button, show_tooltips, label).into_any_element()
}

/// The no-staged-files footer: the commit box only exists when a commit can
/// succeed, so this says why it is missing instead of leaving the column
/// blank. The stage-all verb stands where the commit button would be and wears
/// the same primary fill, so the column's one action reads the same either way.
fn stage_hint(store: Entity<Store>, unstaged: usize) -> gpui::AnyElement {
    let mut footer = div()
        .debug_selector(|| "changes-stage-hint".into())
        .flex_shrink_0()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(theme::hairline())
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .child(
            // wraps rather than truncating: at the column's narrowest the
            // sentence the footer exists for must stay readable
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child("Stage changes to commit"),
        );
    if unstaged > 0 {
        let action_store = store.clone();
        footer = footer.child(crate::ui::primary_button(
            "stage-all-footer",
            "Stage all",
            true,
            move |_, _, cx| {
                action_store.update(cx, |s, cx| s.run_git(GitAction::StageAll, cx));
            },
        ));
    }
    footer.into_any_element()
}

/// The clean working tree: what is true, plus the last commit so the tab still
/// says where the branch stands.
fn clean_state(store: &Entity<Store>, cx: &mut gpui::App) -> gpui::AnyElement {
    let last = store
        .read(cx)
        .recent_commits
        .first()
        .map(|commit| {
            format!(
                "Last commit {}: {}",
                warp_station_core::rel_time::format(commit.date),
                commit.subject
            )
        })
        .or_else(|| {
            store
                .read(cx)
                .selected_entry()
                .and_then(|repo| repo.git_last_commit_at)
                .map(|at| format!("Last commit {}", warp_station_core::rel_time::format(at)))
        });
    div()
        .debug_selector(|| "changes-clean".into())
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .p(px(warpui_kit_core::shell::DETAIL_PADDING))
        .child(crate::ui::icon(
            "icons/icon-check.svg",
            px(theme::icon_large()),
            theme::SUCCESS_BRIGHT,
        ))
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child("Working tree clean"),
        )
        .children(last.map(|last| {
            div()
                .max_w(px(warpui_kit_core::shell::READING_WIDTH))
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(last)
        }))
        .into_any_element()
}

/// The +/− stage-toggle button per row: unstaged rows stage, staged rows
/// unstage.
fn row_action_button(
    file: &GitChangedFile,
    staged: bool,
    store: Entity<Store>,
) -> impl IntoElement + use<> {
    let action = if staged {
        GitAction::Unstage(file.path.clone())
    } else {
        GitAction::Stage(file.path.clone())
    };
    let glyph = if staged {
        "icons/icon-minus.svg"
    } else {
        "icons/icon-plus.svg"
    };
    let selector = format!("toggle-{staged}-{}", file.path);
    div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .a11y_button(if staged {
            format!("Unstage {}", file.path)
        } else {
            format!("Stage {}", file.path)
        })
        .cursor_pointer()
        .rounded_md()
        // the same 24 px as the discard verb beside it: a 20 px toggle read
        // as a smaller control in the row's hover gutter
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme::blue_bright())
        .hover(|s| s.bg(theme::tint(theme::accent())))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(icon(
            glyph,
            px(theme::icon_small() - 2.),
            theme::blue_bright(),
        ))
}

/// A folder row's stage or unstage verb: one git process for the whole
/// subtree, so a twenty-file folder costs one `git add`, not twenty.
fn folder_staging_button(
    staged: bool,
    path: &str,
    label: &str,
    count: usize,
    this: Entity<ChangesView>,
    store: Entity<Store>,
    show_tooltips: bool,
) -> gpui::AnyElement {
    let verb = if staged { "Unstage" } else { "Stage" };
    let name = format!("{verb} {label} ({count} files)");
    let selector = format!("{}-folder-{path}", if staged { "unstage" } else { "stage" });
    let path = path.to_string();
    let button = div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .a11y_button(name.clone())
        .cursor_pointer()
        .rounded_md()
        .size(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme::blue_bright())
        .hover(|s| s.bg(theme::tint(theme::accent())))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let paths = this.update(cx, |this, _| this.folder_paths(&path, staged));
            if paths.is_empty() {
                return;
            }
            let action = if staged {
                GitAction::UnstagePaths(paths)
            } else {
                GitAction::StagePaths(paths)
            };
            store.update(cx, |store, cx| store.run_git(action, cx));
        })
        .child(icon(
            if staged {
                "icons/icon-minus.svg"
            } else {
                "icons/icon-plus.svg"
            },
            px(theme::icon_small() - 2.),
            theme::blue_bright(),
        ));
    crate::ui::with_tooltip(button, show_tooltips, name).into_any_element()
}

/// A folder row's discard verb: the same click-twice arm as a file row, and
/// then one `git restore` for the subtree.
fn folder_discard_button(
    armed: bool,
    path: &str,
    label: &str,
    count: usize,
    this: Entity<ChangesView>,
    store: Entity<Store>,
    show_tooltips: bool,
) -> gpui::AnyElement {
    let name = if armed {
        "Confirm discard".to_string()
    } else {
        format!("Discard {label} ({count} files)")
    };
    let selector = format!("discard-folder-{path}");
    let path = path.to_string();
    let button = div()
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone())
        .a11y_button(name.clone())
        .cursor_pointer()
        .rounded_md()
        .h(px(24.))
        .min_w(px(24.))
        .flex()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(theme::tint(theme::DANGER)))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            if armed {
                this.update(cx, |this, _| this.discard_armed = None);
                let paths = this.update(cx, |this, _| this.folder_paths(&path, false));
                if !paths.is_empty() {
                    store.update(cx, |store, cx| {
                        store.run_git(GitAction::DiscardUnstaged(paths), cx)
                    });
                }
            } else {
                this.update(cx, |this, cx| {
                    this.discard_armed = Some((path.clone(), false));
                    cx.notify();
                });
            }
        })
        .when(armed, |d| {
            d.px_1p5()
                .text_size(px(theme::font_meta()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::DANGER)
                .child("Confirm")
        })
        .when(!armed, |d| {
            d.child(icon(
                "icons/icon-undo.svg",
                px(theme::icon_tiny()),
                theme::text_dim(),
            ))
        });
    crate::ui::with_tooltip(button, show_tooltips, name).into_any_element()
}

// ---- the branch picker ------------------------------------------------------

/// The app's icon paths for the shared picker shell.
const PICKER_ICONS: PickerIcons = PickerIcons {
    check: "icons/icon-check.svg",
    chevron_down: "icons/icon-chevron-down.svg",
    trash: "icons/icon-trash.svg",
    refresh: "icons/icon-refresh.svg",
};

/// The branch picker's ephemeral state: whether the new-branch form replaced
/// the list, and the row's right-click menu (the branch list's other verbs).
#[derive(Default)]
struct BranchUi {
    new_branch_open: bool,
    menu: Option<BranchMenu>,
}

/// The right-click menu on one branch row: the row's value, whether it is a
/// remote-tracking branch, the pointer it opened at, and the delete's
/// two-press arm.
#[derive(Clone)]
struct BranchMenu {
    branch: String,
    is_remote: bool,
    at: gpui::Point<gpui::Pixels>,
    delete_armed: bool,
}

/// The branch selector: a pick checks the branch out, a right-click opens the
/// row's menu, and the popup carries the new-branch form.
fn build_branch_picker(
    store: &Entity<Store>,
    branch_ui: Rc<RefCell<BranchUi>>,
    new_branch_name: Entity<InputState>,
    menu_focus: &FocusHandle,
    window: &mut Window,
    cx: &mut Context<ChangesView>,
) -> Entity<FilteredPicker> {
    let spec = PickerSpec {
        key: "Branch",
        trigger_label: "Branch",
        filter_placeholder: "Filter branches",
        icons: PICKER_ICONS,
        trigger_icon: Some("icons/icon-source-branch.svg"),
        provider_badge: false,
        refresh: false,
        refresh_label: "",
        stop_trigger_mousedown: true,
        visible_rows: 8,
        selected_accent: true,
        overlay: true,
        anchor: OverlayAnchor::Rail,
        trigger_scale: TriggerScale::Compact,
        trigger_rounded_lg: false,
        trigger_min_h: None,
        panel_chrome: PanelChrome::Menus,
        trigger_id: Some("branch-button"),
        panel_id: Some("branch-menu"),
        filter_selector: Some("branch-filter"),
        list_id: Some("branch-list"),
    };
    let pick_store = store.clone();
    let branch_picker = cx.new(|cx| {
        FilteredPicker::new(
            spec,
            PickerTarget::Callback(Box::new(move |value, _window, cx| {
                pick_store.update(cx, |s, cx| {
                    s.run_git(GitAction::Checkout(value.to_string()), cx)
                });
            })),
            Box::new(|_, _, _| unreachable!("the real source lands right after construction")),
            window,
            cx,
        )
    });
    {
        let data_store = store.clone();
        let data_ui = branch_ui.clone();
        let data_name = new_branch_name.clone();
        let data_picker = branch_picker.downgrade();
        branch_picker.update(cx, |picker, _cx| {
            picker.set_data_source(Box::new(
                move |filter: &str, window: &Window, cx: &gpui::App| {
                    // the picker hands itself in for the form's repaints;
                    // a dying entity renders an empty list
                    match data_picker.upgrade() {
                        Some(picker) => branch_data(
                            &data_store,
                            &data_ui,
                            &data_name,
                            &picker,
                            filter,
                            window,
                            cx,
                        ),
                        None => PickerData {
                            body: PickerBody::Rows(Vec::new()),
                            selected: Selection::default(),
                            footer: None,
                            fonts: theme::active_tokens(),
                            accent: theme::blue_bright(),
                            show_tooltips: false,
                            refreshing: false,
                        },
                    }
                },
            ));
        });
    }
    // dismissal (outside click, Escape, a pick) clears the ephemeral state;
    // the row menu is not part of it — it opens over the still-open picker,
    // and closing the menu dismisses the picker
    branch_picker.update(cx, |picker, cx| {
        picker.set_on_dismiss({
            let ui = branch_ui.clone();
            Box::new(move |_cx| {
                ui.borrow_mut().new_branch_open = false;
            })
        });
        picker.set_on_secondary({
            let ui = branch_ui.clone();
            let store = store.clone();
            let focus = menu_focus.clone();
            Rc::new(
                move |name: &str,
                      at: gpui::Point<gpui::Pixels>,
                      window: &mut Window,
                      cx: &mut gpui::App| {
                    let is_remote = store.read(cx).branches.iter().any(|branch| {
                        branch.name == name
                            && branch.kind == warp_station_core::model::GitBranchKind::Remote
                    });
                    ui.borrow_mut().menu = Some(BranchMenu {
                        branch: name.to_string(),
                        is_remote,
                        at,
                        delete_armed: false,
                    });
                    // the catcher answers Escape only while it owns the
                    // keyboard; the focus request lands with this render
                    window.focus(&focus, cx);
                },
            )
        });
        picker.observe_entity(store, cx);
    });
    branch_picker
}

/// One render's worth of the branch list: locals, the inert "Remote" group
/// label, and — when the form is open — the form as the whole panel body.
fn branch_data(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    filter: &str,
    window: &Window,
    cx: &gpui::App,
) -> PickerData {
    let store_read = store.read(cx);
    let current = store_read
        .selected_entry()
        .and_then(|e| e.git_branch_name.clone())
        .unwrap_or_default();
    let show_tooltips = store_read.settings().general.show_tooltips;
    let form_open = branch_ui.borrow().new_branch_open;

    if form_open {
        return PickerData {
            body: PickerBody::Replace(new_branch_form(
                store,
                branch_ui,
                new_branch_name,
                picker,
                window,
                cx,
            )),
            selected: Selection {
                value: current.clone(),
                label: "—".to_string(),
                badge: None,
            },
            footer: None,
            fonts: theme::active_tokens(),
            accent: theme::blue_bright(),
            show_tooltips,
            refreshing: false,
        };
    }

    let filter = filter.to_lowercase();
    let mut rows = Vec::new();
    let mut remotes_labeled = false;
    for branch in &store_read.branches {
        if !filter.is_empty() && !branch.name.to_lowercase().contains(&filter) {
            continue;
        }
        let is_remote = branch.kind == warp_station_core::model::GitBranchKind::Remote;
        // locals first, then the inert "Remote" group label (re-emerges
        // before the first matching remote while filtering)
        if is_remote && !remotes_labeled {
            remotes_labeled = true;
            rows.push(PickerRow::Group {
                id: "branch-group-remote".into(),
                label: "Remote".into(),
                selector: Some("branch-group-remote".into()),
            });
        }
        rows.push(PickerRow::Item(PickerItem {
            id: format!("branch-row-{}", branch.name),
            selector: format!("branch-row-{}", branch.name),
            value: branch.name.clone(),
            // the full name stays the pick payload; the display label sheds
            // its head only when it would overflow
            label: crate::ui::branch_label(&branch.name, window),
            selected: branch.name == current,
            muted: is_remote,
            // the row's verbs (checkout, copy, delete) live in the
            // right-click menu; the trailing slot stays free
            trailing_delete: None,
        }));
    }

    PickerData {
        body: PickerBody::Rows(rows),
        selected: Selection {
            label: if current.is_empty() {
                "—".to_string()
            } else {
                crate::ui::branch_label(&current, window)
            },
            badge: None,
            value: current,
        },
        footer: Some(new_branch_footer(
            store,
            branch_ui,
            new_branch_name,
            picker,
            cx,
        )),
        fonts: theme::active_tokens(),
        accent: theme::blue_bright(),
        show_tooltips,
        refreshing: false,
    }
}

/// The panel footer: "+ New branch…" opens the form seeded with the repo's
/// branch-name prefix and focused, ready to type.
fn new_branch_footer(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    cx: &gpui::App,
) -> gpui::AnyElement {
    let prefix = store.read(cx).settings().repos.branch_name_prefix.clone();
    let seed = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };
    let ui = branch_ui.clone();
    let name = new_branch_name.clone();
    let picker = picker.downgrade();
    div()
        .id("new-branch-row")
        .debug_selector(|| "new-branch-row".into())
        .a11y_button("New branch…")
        .cursor_pointer()
        .rounded_md()
        .px_2()
        .py_1()
        .text_size(px(theme::font_body()))
        .text_color(theme::blue_bright())
        .hover(|s| s.bg(theme::hover_bg()))
        .on_click(move |_, window, cx| {
            // the form always opens seeded with the repo's prefix and
            // focused, ready to type the branch name
            name.update(cx, |state, cx| {
                state.set_value(seed.as_str(), window, cx);
                state.focus(window, cx);
            });
            ui.borrow_mut().new_branch_open = true;
            let _ = picker.update(cx, |_, cx| cx.notify());
        })
        .child("+ New branch…")
        .into_any_element()
}

/// The open new-branch form: the name input over Cancel/Create, replacing the
/// whole panel body.
fn new_branch_form(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    window: &Window,
    cx: &gpui::App,
) -> gpui::AnyElement {
    let cancel_ui = branch_ui.clone();
    let create_ui = branch_ui.clone();
    let create_store = store.clone();
    let create_name = new_branch_name.clone();
    let cancel_picker = picker.downgrade();
    let create_picker = picker.downgrade();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().debug_selector(|| "new-branch-name".into()).child(
            warpui_kit_core::field::mono("new-branch-name-field", new_branch_name, window, cx),
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    div()
                        .id("new-branch-cancel")
                        .debug_selector(|| "new-branch-cancel".into())
                        .a11y_button("Cancel new branch")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::text_dim())
                        .border_1()
                        .border_color(theme::hairline())
                        .hover(|s| s.bg(theme::hover_bg()))
                        .on_click(move |_, _, cx| {
                            cancel_ui.borrow_mut().new_branch_open = false;
                            let _ = cancel_picker.update(cx, |_, cx| cx.notify());
                        })
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id("new-branch-create")
                        .debug_selector(|| "new-branch-create".into())
                        .a11y_button("Create branch")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::MEDIUM)
                        .bg(theme::accent())
                        .text_color(theme::on_accent())
                        .hover(|s| s.opacity(0.9))
                        .on_click(move |_, window, cx| {
                            let name = create_name.read(cx).value().trim().to_string();
                            if name.is_empty() {
                                return;
                            }
                            create_store.update(cx, |s, cx| {
                                s.run_git(
                                    GitAction::CreateBranch {
                                        name,
                                        start: None,
                                        checkout: true,
                                    },
                                    cx,
                                )
                            });
                            // a created branch is a selection: the popup
                            // closes and the form resets
                            create_ui.borrow_mut().new_branch_open = false;
                            create_name.update(cx, |f, cx| f.set_value("", window, cx));
                            let _ = create_picker.update(cx, |p, cx| {
                                p.dismiss(cx);
                            });
                        })
                        .child("Create"),
                ),
        )
        .into_any_element()
}

/// A conflicted path in place of an empty diff: the merge editor is the only
/// way to see and resolve it, so the frame offers that door directly.
fn merge_conflict_state(
    file: &warp_station_core::git::merge::MergeFile,
    store: Entity<Store>,
) -> gpui::AnyElement {
    use warp_station_core::git::merge::MergeFile;
    let detail = match file {
        MergeFile::Text(text) => format!("{} conflict block(s)", text.blocks.len()),
        MergeFile::WholeFile(whole) => whole.reason.label().to_string(),
    };
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div()
                .text_size(px(theme::font_heading()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child("Merge conflict"),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(format!("{} — {detail}", file.path())),
        )
        .child(crate::ui::primary_button(
            "changes-open-merge",
            "Open merge editor",
            true,
            move |_, _, cx| {
                store.update(cx, |store, cx| store.open_merge_editor(cx));
            },
        ))
        .into_any_element()
}
