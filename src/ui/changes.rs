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
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{
    Context, Entity, FocusHandle, FontWeight, HighlightStyle, ListAlignment, ListState,
    MouseButton, ScrollStrategy, SharedString, StyledText, Subscription, UniformListScrollHandle,
    Window, div, list, px, uniform_list,
};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use std::cell::RefCell;
use std::rc::Rc;
use warp_station_core::git::conflict::{
    ConflictPick, ConflictSide, GitConflictFile, assemble_picks_tracked,
};
use warp_station_core::git::merge_diff;
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
}

/// Which of the file list's two sections a header folds.
#[derive(Clone, Copy)]
enum FileSection {
    Staged,
    Unstaged,
}

/// One fixed-height item of the virtualized file list: a section header or a
/// row on one of the two sides.
#[derive(Clone, Copy)]
enum ChangeListItem {
    StagedHeader,
    StagedFile(usize),
    UnstagedHeader,
    UnstagedFile(usize),
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
/// The merge band's height: the flight, the stopped merge, the ready step.
const MERGE_BAND_H: f32 = 56.;
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
    /// The conflicted file row's context menu.
    file_menu: Option<FileMenu>,
    /// The last file-row press, for the 400 ms double-click window.
    last_file_click: Option<(String, std::time::Instant)>,
    /// The Local/Incoming/Result editor, when a conflict row is open.
    merge_editor: Option<MergeEditor>,
    /// The editor's keyboard owner; F7/F10/F11 and Ctrl+Enter live here.
    editor_focus: FocusHandle,
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
    /// The repo the view's local state belongs to; a mismatch resets it all.
    loaded_for: Option<String>,
    /// Staged/unstaged groups, rebuilt only when the store's change snapshot
    /// generation moves (any store notify otherwise re-cloned everything).
    groups_cache: Option<(u64, Rc<FileGroups>)>,
    /// Precomputed history row labels, keyed by history generation + today.
    history_rows: Vec<HistoryRow>,
    history_cache_key: (u64, chrono::NaiveDate),
    /// The file list's keyboard focus.
    file_focus: FocusHandle,
    /// Keeps the keyboard-selected file row in view.
    file_scroll: UniformListScrollHandle,
    /// The history commit list's keyboard focus.
    history_focus: FocusHandle,
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
        Self {
            store,
            selected_file: None,
            diff_view,
            diff_wrap,
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
            file_menu: None,
            last_file_click: None,
            merge_editor: None,
            editor_focus: cx.focus_handle(),
            commit_message,
            history_open: false,
            drilled_commit: None,
            detail_armed: None,
            detail_patches: std::collections::HashMap::new(),
            detail_requests: std::collections::HashSet::new(),
            commit_file: 0,
            loaded_for: None,
            groups_cache: None,
            history_rows: Vec::new(),
            history_cache_key: (0, chrono::NaiveDate::MIN),
            file_focus: cx.focus_handle(),
            file_scroll: UniformListScrollHandle::default(),
            history_focus: cx.focus_handle(),
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
        self.file_menu = None;
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
        cx.notify();
    }

    /// Opens the history list, loading it once per selection.
    pub fn open_history(&mut self, cx: &mut Context<Self>) {
        let needs_load = self.store.read(cx).history().is_empty();
        self.history_open = true;
        self.drilled_commit = None;
        self.commit_file = 0;
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
        self.ensure_commit_file_selected(cx);
        cx.notify();
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

    /// Shows a file's diff, loading its patch the first time.
    /// A plain press selects the row; a second press on the same conflicted
    /// row within 400 ms opens its editor.
    fn click_file(
        &mut self,
        path: &str,
        staged: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let double = self.last_file_click.as_ref().is_some_and(|(last, at)| {
            last == path && at.elapsed() < std::time::Duration::from_millis(400)
        });
        self.last_file_click = Some((path.to_string(), std::time::Instant::now()));
        self.select_file(path, staged, cx);
        if double {
            let conflicted =
                self.store.read(cx).merge_state().is_some_and(|merge| {
                    merge.conflicted.iter().any(|conflicted| conflicted == path)
                });
            if conflicted {
                self.open_merge_editor(path.to_string(), window, cx);
            }
        }
    }

    fn select_file(&mut self, path: &str, staged: bool, cx: &mut Context<Self>) {
        let key = (path.to_string(), staged);
        self.selected_file = Some(key.clone());
        if let Some(folder) = self.store.read(cx).selected_repo.clone() {
            self.file_by_repo.insert(folder, key.clone());
        }
        if !self.patches.contains_key(&key)
            && let Some(folder) = self.store.read(cx).selected_repo.clone()
        {
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
        cx.notify();
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
            self.groups_cache = Some((
                generation,
                Rc::new(FileGroups {
                    groups,
                    unstaged_paths,
                }),
            ));
            // a new snapshot can add or drop files under an armed discard-all,
            // so the arming never outlives the tree it was given for
            self.discard_all_armed = false;
            // a new snapshot means the fetched patches may be stale
            self.patches.clear();
            if let Some((path, staged)) = self.selected_file.clone() {
                self.select_file(&path, staged, cx);
            }
        }
        let file_groups = self.groups_cache.as_ref().expect("groups cached").1.clone();
        let groups = &file_groups.groups;
        self.ensure_file_selected(groups, cx);
        self.ensure_commit_file_selected(cx);
        let band = self.render_merge_band(cx);
        // an abort (or a resolution outside the app) ends the merge while the
        // editor is still up: the editor follows the repository, not itself
        if self.merge_editor.is_some() && self.store.read(cx).merge_state().is_none() {
            self.merge_editor = None;
        }
        if self.merge_editor.is_some() {
            let editor = self.render_merge_editor(window, cx);
            return div()
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .children(band)
                .child(editor);
        }

        if self.history_open {
            self.pane_nav.register(&self.history_focus);
            return div()
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .children(band)
                .child(
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
        // A resolved merge leaves a clean tree with MERGE_HEAD still in place:
        // the band owns the finish, so the empty state must not claim it.
        let merging = self.store.read(cx).merge_state().is_some();
        let clean = groups.0.is_empty()
            && groups.1.is_empty()
            && self.store.read(cx).changes_loaded_for.is_some()
            && !merging;
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
                        ))
                        .child(file_list),
                );
            files = if merging {
                // the band's Continue is the one way to finish a merge; a
                // second commit box here would take a message of its own
                files.child(merge_commit_hint())
            } else if groups.0.is_empty() && !committing {
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
        // wrapper is the positioned ancestor, so a merge band above the
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
            .children(band)
            .child(toolbar_and_popup)
            .child(body)
            // the row menus paint last: they are the tab's topmost layer
            // while they are up
            .children(self.render_branch_menu(cx))
            .children(self.render_file_menu(cx))
    }
}

impl ChangesView {
    /// A conflicted file's pane: the merge editor is where it gets resolved,
    /// so the pane says so and opens it. The normal patch read is skipped —
    /// an unmerged path has no two-side patch to draw.
    fn render_conflict_pane(
        &mut self,
        path: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let open_path = path.to_string();
        div()
            .id("changes-diff")
            .debug_selector(|| "changes-diff".into())
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
                    .px(px(warpui_kit_core::shell::DETAIL_PADDING))
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(theme::hairline())
                    .child(icon(
                        "icons/icon-file-diff.svg",
                        px(theme::icon_small()),
                        theme::DANGER,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(path.to_string()),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .p(px(warpui_kit_core::shell::DETAIL_PADDING))
                    .child(icon(
                        "icons/icon-alert.svg",
                        px(theme::icon_large()),
                        theme::DANGER,
                    ))
                    .child(
                        div()
                            .text_size(px(theme::font_heading()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text())
                            .child("This file has merge conflicts"),
                    )
                    .child(
                        div()
                            .max_w(px(420.))
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .child(
                                "Pick Local or Incoming for each block in the merge editor, \
                                 then Accept Merge to stage the result.",
                            ),
                    )
                    .child(
                        div()
                            .id("open-merge-editor")
                            .debug_selector(|| "open-merge-editor".into())
                            .a11y_button("Open merge editor")
                            .cursor_pointer()
                            .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                            .px_3()
                            .rounded(px(6.))
                            .bg(theme::accent())
                            .text_color(theme::on_accent())
                            .flex()
                            .items_center()
                            .text_size(px(theme::font_control()))
                            .font_weight(FontWeight::MEDIUM)
                            .hover(|style| style.opacity(0.92))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_merge_editor(open_path.clone(), window, cx);
                            }))
                            .child("Open merge editor"),
                    ),
            )
            .into_any_element()
    }

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
        let conflicted = self
            .store
            .read(cx)
            .merge_state()
            .is_some_and(|merge| merge.conflicted.iter().any(|path| path == &key.0));
        if conflicted {
            return self.render_conflict_pane(&key.0, window, cx);
        }
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
        // an empty patch is a state, not a one-row diff: a binary file or a
        // mode-only change has nothing textual to draw
        let has_patch = matches!(patch, Some(patch) if !patch.is_empty());
        let body: gpui::AnyElement = match patch {
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

    /// The file list flattened to its fixed-height items, in render order.
    /// The renderer and the keyboard cursor both read this one order, so a
    /// collapsed section or a new snapshot moves them together.
    fn file_items(&self, files: &FileGroups) -> Vec<ChangeListItem> {
        let (staged, unstaged) = (&files.groups.0, &files.groups.1);
        let mut items: Vec<ChangeListItem> = Vec::with_capacity(staged.len() + unstaged.len() + 2);
        if !staged.is_empty() {
            items.push(ChangeListItem::StagedHeader);
            if !self.staged_collapsed {
                items.extend((0..staged.len()).map(ChangeListItem::StagedFile));
            }
        }
        if !unstaged.is_empty() {
            items.push(ChangeListItem::UnstagedHeader);
            if !self.unstaged_collapsed {
                items.extend((0..unstaged.len()).map(ChangeListItem::UnstagedFile));
            }
        }
        items
    }

    /// The file list's keys, dispatched by the focused list only.
    fn handle_file_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(dir) = pane_nav::direction(event) {
            return self.pane_nav.move_from(&self.file_focus, dir, window, cx);
        }
        if event.keystroke.modifiers.modified() || self.history_open {
            return false;
        }
        let Some(key) = list_nav::list_key(event) else {
            return false;
        };
        match key {
            list_nav::ListKey::Enter => {
                let conflicted = self.selected_file.as_ref().and_then(|(path, _)| {
                    let merge = self.store.read(cx).merge_state()?;
                    merge
                        .conflicted
                        .iter()
                        .any(|conflicted| conflicted == path)
                        .then(|| path.clone())
                });
                match conflicted {
                    Some(path) => {
                        self.open_merge_editor(path, window, cx);
                        true
                    }
                    None => false,
                }
            }
            list_nav::ListKey::Up
            | list_nav::ListKey::Down
            | list_nav::ListKey::Home
            | list_nav::ListKey::End
            | list_nav::ListKey::PageUp
            | list_nav::ListKey::PageDown => {
                self.move_file_selection(key, cx);
                true
            }
            _ => false,
        }
    }

    /// Steps the selected file through the visible rows: headers are skipped
    /// and a collapsed section has no rows to land on.
    fn move_file_selection(&mut self, key: list_nav::ListKey, cx: &mut Context<Self>) {
        let files = {
            let Some((_, files)) = &self.groups_cache else {
                return;
            };
            files.clone()
        };
        let (staged, unstaged) = (&files.groups.0, &files.groups.1);
        let mut rows: Vec<(String, bool, usize)> = Vec::new();
        for (ix, item) in self.file_items(&files).into_iter().enumerate() {
            match item {
                ChangeListItem::StagedFile(index) => {
                    rows.push((staged[index].path.clone(), true, ix));
                }
                ChangeListItem::UnstagedFile(index) => {
                    rows.push((unstaged[index].path.clone(), false, ix));
                }
                ChangeListItem::StagedHeader | ChangeListItem::UnstagedHeader => {}
            }
        }
        let current = self.selected_file.as_ref().and_then(|(path, is_staged)| {
            rows.iter()
                .position(|(row_path, row_staged, _)| row_path == path && row_staged == is_staged)
        });
        let Some(next) = list_nav::step(current, rows.len(), key, list_nav::DEFAULT_PAGE) else {
            return;
        };
        let (path, is_staged, item_ix) = rows[next].clone();
        self.select_file(&path, is_staged, cx);
        self.file_scroll
            .scroll_to_item(item_ix, ScrollStrategy::Nearest);
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
            list_nav::ListKey::Enter => {
                let conflicted = self.selected_file.as_ref().and_then(|(path, _)| {
                    let merge = self.store.read(cx).merge_state()?;
                    merge
                        .conflicted
                        .iter()
                        .any(|conflicted| conflicted == path)
                        .then(|| path.clone())
                });
                match conflicted {
                    Some(path) => {
                        self.open_merge_editor(path, window, cx);
                        true
                    }
                    None => false,
                }
            }
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
        let items = Rc::new(self.file_items(files));
        let files = files.clone();
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        uniform_list(
            SharedString::from("changes-list"),
            items.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                let entity = cx.entity();
                let (staged, unstaged) = (&files.groups.0, &files.groups.1);
                range
                    .map(|ix| match items[ix] {
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
                        ChangeListItem::StagedFile(index) => {
                            this.render_row(&staged[index], true, cx)
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
                        ChangeListItem::UnstagedFile(index) => {
                            this.render_row(&unstaged[index], false, cx)
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

    fn render_row(
        &mut self,
        file: &GitChangedFile,
        staged: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = (file.path.clone(), staged);
        let selected = self.selected_file.as_ref() == Some(&key);
        let armed = self.discard_armed.as_ref() == Some(&key);
        let status_color = diff_controls::status_color(file.status_code.as_str());
        let row_id: SharedString =
            format!("{}-{}", if staged { "s" } else { "u" }, file.path).into();
        let row_selector = row_id.clone();
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (dir.to_string(), name.to_string()),
            None => (String::new(), file.path.clone()),
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
            .pl(px(13.))
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
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let path = file.path.clone();
                    move |this, event: &gpui::MouseDownEvent, window, cx| {
                        let conflicted = this.store.read(cx).merge_state().is_some_and(|merge| {
                            merge
                                .conflicted
                                .iter()
                                .any(|conflicted| conflicted == &path)
                        });
                        if !conflicted {
                            return;
                        }
                        cx.stop_propagation();
                        this.file_menu = Some(FileMenu {
                            path: path.clone(),
                            at: event.position,
                        });
                        window.focus(&this.menu_focus, cx);
                        cx.notify();
                    }
                }),
            )
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
                    .child(div().flex_shrink_0().text_color(theme::text()).child(name))
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(theme::text_muted())
                            .child(dir),
                    ),
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
        // clone the whole file list on every repaint
        let rail_files = details.clone();
        let rows = uniform_list(
            SharedString::from("commit-files"),
            details.files.len(),
            cx.processor(move |this, range: std::ops::Range<usize>, _window, cx| {
                range
                    .map(|index| this.render_commit_file_row(&rail_files, index, selected_file, cx))
                    .collect()
            }),
        )
        .flex_1()
        .min_h_0()
        .py_1()
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
    fn render_commit_file_row(
        &mut self,
        details: &std::sync::Arc<warp_station_core::model::GitCommitDetails>,
        index: usize,
        selected_file: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let file = &details.files[index];
        let selected = index == selected_file;
        let (dir, name) = match file.path.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name.to_string()),
            None => (String::new(), file.path.clone()),
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
            .pl(px(13.))
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
            .on_click(cx.listener(move |this, _, _, cx| this.select_commit_file(index, cx)))
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
                    .child(div().flex_shrink_0().text_color(theme::text()).child(name))
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

    /// The merge band: the flight, the stopped merge with its conflict count,
    /// and the ready-to-commit step. It sits above the toolbar (and above the
    /// merge editor) because the merge is the tab's state, not the list's.
    fn render_merge_band(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let store = self.store.read(cx);
        let running = store.running_job() == Some("merge");
        let merge = store.merge_state().cloned();
        if !running && merge.is_none() {
            return None;
        }
        let branch = store
            .selected_entry()
            .and_then(|entry| entry.git_branch_name.clone())
            .filter(|branch| !branch.is_empty() && branch != "(detached)")
            .unwrap_or_else(|| "the current branch".to_string());
        let source = if running {
            store.merge_running_source().map(str::to_string)
        } else {
            merge
                .as_ref()
                .map(|merge| merge.source.clone())
                .filter(|source| !source.is_empty())
        };
        let conflicts = merge.as_ref().map_or(0, |merge| merge.conflicted.len());
        let busy = store.git_busy();

        let (headline, detail) = if running {
            (
                match source.as_deref() {
                    Some(source) => format!("Merging {source} into {branch}…"),
                    None => format!("Merging into {branch}…"),
                },
                "The worktree is untouched until git answers.".to_string(),
            )
        } else if conflicts > 0 {
            (
                match source.as_deref() {
                    Some(source) => format!("Merging {source} into {branch}"),
                    None => format!("Merge in progress on {branch}"),
                },
                if conflicts == 1 {
                    "1 file needs attention".to_string()
                } else {
                    format!("{conflicts} files need attention")
                },
            )
        } else {
            (
                "Merge ready to commit".to_string(),
                "Every conflict is resolved; Continue lands the merge commit.".to_string(),
            )
        };

        let mut row = div()
            .id("merge-band")
            .debug_selector(|| "merge-band".into())
            .flex_shrink_0()
            .w_full()
            .min_h(px(MERGE_BAND_H))
            .px(px(warpui_kit_core::shell::DETAIL_PADDING))
            .py(px(8.))
            .bg(theme::surface())
            .border_b_1()
            .border_color(theme::hairline())
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.));

        row = row.child(if running {
            warpui_kit_core::motion::spinner(
                "merge-band-spinner",
                "icons/icon-spinner.svg",
                px(theme::icon_small()),
                theme::text_dim(),
            )
        } else if conflicts > 0 {
            icon(
                "icons/icon-alert.svg",
                px(theme::icon_small()),
                theme::DANGER,
            )
            .into_any_element()
        } else {
            icon(
                "icons/icon-check.svg",
                px(theme::icon_small()),
                theme::SUCCESS_BRIGHT,
            )
            .into_any_element()
        });
        row = row.child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(theme::font_control()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme::text())
                        .child(headline),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_dim())
                        .child(detail),
                ),
        );

        if !running {
            let continue_store = self.store.clone();
            let conflicts_left = conflicts > 0;
            row = row.child(crate::ui::primary_button(
                "merge-continue",
                "Continue",
                !conflicts_left && !busy,
                move |_, _, cx| {
                    continue_store.update(cx, |store, cx| {
                        store.run_git(GitAction::MergeContinue, cx);
                    });
                },
            ));
            let abort_store = self.store.clone();
            row = row.child(
                div()
                    .id("merge-abort")
                    .debug_selector(|| "merge-abort".into())
                    .a11y_button("Abort the merge")
                    .flex_shrink_0()
                    .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                    .px_3()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(theme::line_strong())
                    .bg(theme::panel())
                    .flex()
                    .items_center()
                    .text_size(px(theme::font_control()))
                    .font_weight(FontWeight::MEDIUM)
                    .when(!busy, |button| {
                        button
                            .cursor_pointer()
                            .text_color(theme::text())
                            .hover(|style| style.bg(theme::raised()))
                            .on_click(move |_, _, cx| {
                                abort_store.update(cx, |store, cx| {
                                    store.run_git(GitAction::MergeAbort, cx);
                                });
                            })
                    })
                    .when(busy, |button| {
                        button.opacity(0.45).text_color(theme::text_dim())
                    })
                    .child("Abort"),
            );
        }

        Some(row.into_any_element())
    }

    /// The branch row's context menu: merge the row into the current branch,
    /// check it out, copy its name, delete it. It opens at the pointer, closes
    /// on any press outside, on Escape, and after an action.
    fn render_branch_menu(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let menu = self.branch_ui.borrow().menu.clone()?;
        let (current, busy, merging) = {
            let store = self.store.read(cx);
            (
                store
                    .selected_entry()
                    .and_then(|entry| entry.git_branch_name.clone())
                    .unwrap_or_default(),
                store.git_busy(),
                store.merge_state().is_some(),
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

        // Merge into the current branch: the one verb this menu exists for.
        let store = self.store.clone();
        let merge_source = branch.clone();
        let merge_close = close.clone();
        card = card.child(crate::ui::pull_request::menu_item(
            "branch-menu-merge",
            "icons/icon-arrow-down.svg",
            "Merge into current branch",
            !is_current && !busy && !merging,
            move |_, _, cx| {
                store.update(cx, |store, cx| {
                    store.run_git(
                        GitAction::Merge {
                            source: merge_source.clone(),
                        },
                        cx,
                    );
                });
                merge_close(cx);
            },
        ));

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

    fn close_file_menu(&mut self, cx: &mut Context<Self>) {
        if self.file_menu.take().is_some() {
            cx.notify();
        }
    }

    /// The conflicted row's menu: open the editor, or resolve every block to
    /// one side in one press. Only conflicted rows get it; the row's left
    /// click keeps selecting.
    fn render_file_menu(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let menu = self.file_menu.clone()?;
        let still_conflicted = self.store.read(cx).merge_state().is_some_and(|merge| {
            merge
                .conflicted
                .iter()
                .any(|conflicted| conflicted == &menu.path)
        });
        if !still_conflicted {
            self.file_menu = None;
            return None;
        }
        let path = menu.path.clone();
        let mut card = crate::ui::pull_request::menu_shell("file-row-menu");
        card = card.child(
            div()
                .px_2()
                .py_1()
                .truncate()
                .text_size(px(theme::font_meta()))
                .text_color(theme::text_muted())
                .child(path.clone()),
        );
        let open_path = path.clone();
        card = card.child(crate::ui::pull_request::menu_item(
            "file-menu-open",
            "icons/icon-file-diff.svg",
            "Open merge editor",
            true,
            cx.listener(move |this, _, window, cx| {
                this.close_file_menu(cx);
                this.open_merge_editor(open_path.clone(), window, cx);
            }),
        ));
        let local_store = self.store.clone();
        let local_path = path.clone();
        card = card.child(crate::ui::pull_request::menu_item(
            "file-menu-local",
            "icons/icon-arrow-left.svg",
            "Resolve with Local",
            true,
            cx.listener(move |this, _, _, cx| {
                this.close_file_menu(cx);
                local_store.update(cx, |store, cx| {
                    store.bulk_resolve_conflict(
                        &local_path,
                        warp_station_core::git::conflict::ConflictSide::Local,
                        cx,
                    );
                });
            }),
        ));
        let incoming_store = self.store.clone();
        let incoming_path = path.clone();
        card = card.child(crate::ui::pull_request::menu_item(
            "file-menu-incoming",
            "icons/icon-arrow-down.svg",
            "Resolve with Incoming",
            true,
            cx.listener(move |this, _, _, cx| {
                this.close_file_menu(cx);
                incoming_store.update(cx, |store, cx| {
                    store.bulk_resolve_conflict(
                        &incoming_path,
                        warp_station_core::git::conflict::ConflictSide::Incoming,
                        cx,
                    );
                });
            }),
        ));

        let catcher = div()
            .id("file-menu-catcher")
            .debug_selector(|| "file-menu-catcher".into())
            .absolute()
            .inset_0()
            .occlude()
            .track_focus(&self.menu_focus)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.close_file_menu(cx);
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.close_file_menu(cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _, _, cx| this.close_file_menu(cx)),
            )
            .child(
                gpui::anchored()
                    .anchor(gpui::Anchor::TopLeft)
                    .position(menu.at)
                    .offset(gpui::point(px(6.), px(6.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(card),
            );
        Some(catcher.into_any_element())
    }

    // ---- the merge editor --------------------------------------------------

    /// Opens the merge editor on one conflicted path and asks the net thread
    /// for its stages and blocks.
    fn open_merge_editor(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.merge_editor = Some(MergeEditor {
            path: path.clone(),
            choices: Vec::new(),
            current: 0,
            whole: None,
            layout: MergeLayout::Split,
            show_base: false,
            diff: Vec::new(),
            diff_scroll: UniformListScrollHandle::default(),
            scrolled: false,
            result: String::new(),
            result_ranges: Vec::new(),
            result_input: None,
            result_subscription: None,
            manual_blocks: Vec::new(),
            last_change_before: String::new(),
            preview: false,
            ready: false,
        });
        self.store
            .update(cx, |store, cx| store.load_merge_conflict(&path, cx));
        window.focus(&self.editor_focus, cx);
        cx.notify();
    }

    fn close_merge_editor(&mut self, cx: &mut Context<Self>) {
        if self.merge_editor.take().is_some() {
            cx.notify();
        }
    }

    fn step_conflict(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(editor) = self.merge_editor.as_mut() else {
            return;
        };
        let total = editor.choices.len();
        if total == 0 {
            return;
        }
        editor.current = (editor.current as isize + delta).clamp(0, total as isize - 1) as usize;
        self.scroll_to_current(cx);
        cx.notify();
    }

    /// Jumps to one block: the panes scroll their own file to the block's
    /// mapped lines, so the three sides read the same conflict.
    fn set_current(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(editor) = self.merge_editor.as_mut() else {
            return;
        };
        if index < editor.choices.len() {
            editor.current = index;
        }
        self.scroll_to_current(cx);
        cx.notify();
    }

    fn scroll_to_current(&self, cx: &gpui::App) {
        let Some(editor) = self.merge_editor.as_ref() else {
            return;
        };
        let Some(file) = self
            .store
            .read(cx)
            .merge_conflict()
            .filter(|file| file.path == editor.path)
        else {
            return;
        };
        let Some(block) = file.blocks.get(editor.current) else {
            return;
        };
        // the block's first row in the aligned list: its own side's first
        // line, or the base's where one side deleted the section
        let row = editor.diff.iter().position(|row| {
            row.local
                .is_some_and(|line| block.local_lines.contains(&line))
                || row
                    .incoming
                    .is_some_and(|line| block.incoming_lines.contains(&line))
                || row
                    .base
                    .is_some_and(|line| block.base_lines.contains(&line))
        });
        if let Some(row) = row {
            editor.diff_scroll.scroll_to_item(row, ScrollStrategy::Top);
        }
    }

    fn toggle_layout(&mut self, cx: &mut Context<Self>) {
        let wide = self.store.read(cx).pane_width > 1000.;
        if let Some(editor) = self.merge_editor.as_mut() {
            editor.layout = match editor.layout {
                MergeLayout::Split if wide => MergeLayout::Columns,
                _ => MergeLayout::Split,
            };
        }
        cx.notify();
    }

    fn toggle_base(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.merge_editor.as_mut() {
            editor.show_base = !editor.show_base;
        }
        cx.notify();
    }

    /// The open editor's conflicted file, when the store has it.
    fn editor_file(&self, cx: &gpui::App) -> Option<GitConflictFile> {
        let editor = self.merge_editor.as_ref()?;
        self.store
            .read(cx)
            .merge_conflict()
            .filter(|file| file.path == editor.path)
            .cloned()
    }

    /// Mirrors `result` into the editable area without treating the write as
    /// a user edit: the change handler ignores a value equal to `result`.
    fn push_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.merge_editor.as_ref() else {
            return;
        };
        let Some(input) = editor.result_input.clone() else {
            return;
        };
        let text = editor.result.clone();
        input.update(cx, |state, cx| state.set_value(&text, window, cx));
    }

    /// Checks a side for the current block (or the whole file when it has no
    /// blocks); checking the chosen side again puts the markers back.
    fn choose_side(&mut self, side: ConflictSide, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.merge_editor.as_ref() else {
            return;
        };
        if editor.choices.is_empty() {
            let whole = editor.whole;
            if let Some(editor) = self.merge_editor.as_mut() {
                editor.whole = if whole == Some(side) {
                    None
                } else {
                    Some(side)
                };
            }
            cx.notify();
            return;
        }
        let index = editor.current.min(editor.choices.len() - 1);
        self.choose_block(index, side, window, cx);
    }

    /// Checks one side of one block, wherever the block was clicked: the diff
    /// rows and the column heads share this.
    fn choose_block(
        &mut self,
        index: usize,
        side: ConflictSide,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pick = match side {
            ConflictSide::Local => ConflictPick::Local,
            ConflictSide::Incoming => ConflictPick::Incoming,
        };
        let Some(editor) = self.merge_editor.as_mut() else {
            return;
        };
        if index >= editor.choices.len() {
            return;
        }
        let already = editor.choices.get(index).copied().flatten();
        let next = if already == Some(pick) {
            None
        } else {
            Some(pick)
        };
        editor.current = index;
        self.take_block(index, next, window, cx);
    }

    /// Replaces one block's range in the Result with its pick (or the original
    /// marker text when clearing) and shifts every later block's range.
    fn take_block(
        &mut self,
        index: usize,
        pick: Option<ConflictPick>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.editor_file(cx) else {
            return;
        };
        let Some(block) = file.blocks.get(index) else {
            return;
        };
        let text = match pick {
            Some(pick) => pick.text(block),
            None => file.worktree[block.range.clone()].to_string(),
        };
        let Some(editor) = self.merge_editor.as_mut() else {
            return;
        };
        let Some(range) = editor.result_ranges.get(index).cloned() else {
            return;
        };
        if range.end > editor.result.len()
            || !editor.result.is_char_boundary(range.start)
            || !editor.result.is_char_boundary(range.end)
        {
            return;
        }
        let delta = text.len() as isize - (range.end - range.start) as isize;
        let mut result = String::with_capacity(editor.result.len().saturating_add_signed(delta));
        result.push_str(&editor.result[..range.start]);
        result.push_str(&text);
        result.push_str(&editor.result[range.end..]);
        for (ix, other) in editor.result_ranges.iter_mut().enumerate() {
            if ix == index {
                *other = range.start..range.start + text.len();
            } else if other.start >= range.end {
                other.start = other.start.saturating_add_signed(delta);
                other.end = other.end.saturating_add_signed(delta);
            }
        }
        editor.choices[index] = pick;
        editor.manual_blocks[index] = false;
        editor.result = result;
        self.push_result(window, cx);
        cx.notify();
    }

    /// Take both: replaces the range with the two sides in the given order.
    fn take_both(&mut self, incoming_first: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.merge_editor.as_ref() else {
            return;
        };
        let index = editor.current.min(editor.choices.len().saturating_sub(1));
        let pick = if incoming_first {
            ConflictPick::IncomingThenLocal
        } else {
            ConflictPick::LocalThenIncoming
        };
        self.take_block(index, Some(pick), window, cx);
    }

    fn take_all(&mut self, side: ConflictSide, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.editor_file(cx) else {
            return;
        };
        if file.blocks.is_empty() {
            if let Some(editor) = self.merge_editor.as_mut() {
                editor.whole = Some(side);
            }
            cx.notify();
            return;
        }
        let pick = match side {
            ConflictSide::Local => ConflictPick::Local,
            ConflictSide::Incoming => ConflictPick::Incoming,
        };
        let picks = vec![Some(pick); file.blocks.len()];
        let (result, ranges) = assemble_picks_tracked(&file.worktree, &file.blocks, &picks);
        let Some(editor) = self.merge_editor.as_mut() else {
            return;
        };
        editor.choices = picks;
        editor.manual_blocks = vec![false; file.blocks.len()];
        editor.result = result;
        editor.result_ranges = ranges;
        self.push_result(window, cx);
        cx.notify();
    }

    /// Puts the marker-bearing worktree text back and clears every pick.
    fn reset_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(file) = self.editor_file(cx) else {
            return;
        };
        let Some(editor) = self.merge_editor.as_mut() else {
            return;
        };
        editor.result = file.worktree.clone();
        editor.result_ranges = file
            .blocks
            .iter()
            .map(|block| block.range.clone())
            .collect();
        editor.choices.fill(None);
        editor.manual_blocks.fill(false);
        editor.whole = None;
        self.push_result(window, cx);
        cx.notify();
    }

    /// A user edit in the Result: remaps every block range through the
    /// smallest changed span; a block the edit touches becomes manual and
    /// loses its pick.
    fn on_result_edited(&mut self, value: String, cx: &mut Context<Self>) {
        let Some(editor) = self.merge_editor.as_mut() else {
            return;
        };
        if value == editor.result {
            return;
        }
        editor.last_change_before = editor.result.clone();
        let (start, old_end, new_end) = minimal_edit(&editor.result, &value);
        for (index, range) in editor.result_ranges.iter_mut().enumerate() {
            let overlaps = !(range.end <= start || range.start >= old_end);
            map_range(range, start, old_end, new_end);
            if overlaps {
                if let Some(choice) = editor.choices.get_mut(index) {
                    *choice = None;
                }
                if let Some(manual) = editor.manual_blocks.get_mut(index) {
                    *manual = true;
                }
            }
        }
        editor.result = value;
        cx.notify();
    }

    /// Writes the Result as it stands and stages it; the reload closes the
    /// row. A result still holding markers is refused.
    /// The merge editor's fallback for a file with no text surface: take one
    /// side through the index stages.
    fn resolve_merge_side_ui(&mut self, path: &str, side: ConflictSide, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.resolve_merge_side(path, side, cx);
        });
        self.merge_editor = None;
        cx.notify();
    }

    /// Keep the working tree's bytes and stage them (the row's own Add verb).
    fn stage_merge_worktree(&mut self, path: &str, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.run_git(GitAction::Stage(path.to_string()), cx);
        });
        self.merge_editor = None;
        cx.notify();
    }

    fn accept_merge(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.merge_editor.as_ref() else {
            return;
        };
        let path = editor.path.clone();
        let Some(file) = self
            .store
            .read(cx)
            .merge_conflict()
            .filter(|file| file.path == path)
            .cloned()
        else {
            return;
        };
        let text = if file.blocks.is_empty() {
            editor.whole.map(|side| match side {
                ConflictSide::Local => file.local.clone(),
                ConflictSide::Incoming => file.incoming.clone(),
            })
        } else if editor.ready && !holds_conflict_markers(&editor.result) {
            Some(editor.result.clone())
        } else {
            None
        };
        let Some(text) = text else {
            return;
        };
        self.store.update(cx, |store, cx| {
            store.resolve_merge_conflict(&path, Some(text), cx);
        });
        self.merge_editor = None;
        cx.notify();
    }

    fn handle_merge_editor_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key == "escape" {
            self.close_merge_editor(cx);
            return true;
        }
        if event.keystroke.modifiers.control && event.keystroke.key == "enter" {
            self.accept_merge(cx);
            return true;
        }
        match event.keystroke.key.to_lowercase().as_str() {
            "f7" => {
                self.step_conflict(
                    if event.keystroke.modifiers.shift {
                        -1
                    } else {
                        1
                    },
                    cx,
                );
                true
            }
            "f10" => {
                self.take_all(ConflictSide::Incoming, _window, cx);
                true
            }
            "f11" => {
                self.take_all(ConflictSide::Local, _window, cx);
                true
            }
            _ => false,
        }
    }

    fn render_merge_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(path) = self.merge_editor.as_ref().map(|editor| editor.path.clone()) else {
            return div().into_any_element();
        };
        let file = self
            .store
            .read(cx)
            .merge_conflict()
            .filter(|file| file.path == path)
            .cloned();
        let conflict_loaded = self.store.read(cx).merge_conflict_loaded(&path);
        let Some(file) = file else {
            return div()
                .id("merge-editor")
                .debug_selector(|| "merge-editor".into())
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .child(
                    div()
                        .flex_shrink_0()
                        .h(px(TOOLBAR_H))
                        .px(px(warpui_kit_core::shell::DETAIL_PADDING))
                        .border_b_1()
                        .border_color(theme::hairline())
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("merge-back")
                                .debug_selector(|| "merge-back".into())
                                .a11y_button("Back to the file list")
                                .cursor_pointer()
                                .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                                .px_2()
                                .rounded(px(6.))
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(theme::font_control()))
                                .text_color(theme::text_muted())
                                .hover(|style| style.bg(theme::raised()).text_color(theme::text()))
                                .on_click(cx.listener(|this, _, _, cx| this.close_merge_editor(cx)))
                                .child(icon(
                                    "icons/icon-arrow-left.svg",
                                    px(theme::icon_small()),
                                    theme::text_dim(),
                                ))
                                .child("Files"),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(theme::font_control()))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme::text())
                                .child(path.to_string()),
                        ),
                )
                .child(if conflict_loaded {
                    let path_local = path.clone();
                    let path_incoming = path.clone();
                    let path_add = path.clone();
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .w(px(440.))
                                .p_4()
                                .rounded(px(8.))
                                .border_1()
                                .border_color(theme::hairline())
                                .bg(theme::panel())
                                .flex()
                                .flex_col()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(theme::font_body()))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme::text())
                                        .child("No text to merge"),
                                )
                                .child(
                                    div()
                                        .text_size(px(theme::font_meta()))
                                        .text_color(theme::text_muted())
                                        .child(
                                            "This file is binary, or one side deleted it.                                              Take a side, or keep the working tree and stage it.",
                                        ),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_2()
                                        .child(
                                            div()
                                                .id("merge-side-local")
                                                .debug_selector(|| "merge-side-local".into())
                                                .a11y_button("Take Local")
                                                .cursor_pointer()
                                                .h(px(28.))
                                                .px_2()
                                                .rounded(px(6.))
                                                .border_1()
                                                .border_color(theme::line_strong())
                                                .flex()
                                                .items_center()
                                                .text_size(px(theme::font_control()))
                                                .text_color(theme::text())
                                                .hover(|style| style.bg(theme::raised()))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.resolve_merge_side_ui(
                                                        &path_local,
                                                        ConflictSide::Local,
                                                        cx,
                                                    );
                                                }))
                                                .child("Take Local"),
                                        )
                                        .child(
                                            div()
                                                .id("merge-side-incoming")
                                                .debug_selector(|| "merge-side-incoming".into())
                                                .a11y_button("Take Incoming")
                                                .cursor_pointer()
                                                .h(px(28.))
                                                .px_2()
                                                .rounded(px(6.))
                                                .border_1()
                                                .border_color(theme::line_strong())
                                                .flex()
                                                .items_center()
                                                .text_size(px(theme::font_control()))
                                                .text_color(theme::text())
                                                .hover(|style| style.bg(theme::raised()))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.resolve_merge_side_ui(
                                                        &path_incoming,
                                                        ConflictSide::Incoming,
                                                        cx,
                                                    );
                                                }))
                                                .child("Take Incoming"),
                                        )
                                        .child(
                                            div()
                                                .id("merge-side-add")
                                                .debug_selector(|| "merge-side-add".into())
                                                .a11y_button("Keep the working tree")
                                                .cursor_pointer()
                                                .h(px(28.))
                                                .px_2()
                                                .rounded(px(6.))
                                                .border_1()
                                                .border_color(theme::line_strong())
                                                .flex()
                                                .items_center()
                                                .text_size(px(theme::font_control()))
                                                .text_color(theme::text_muted())
                                                .hover(|style| style.bg(theme::raised()))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.stage_merge_worktree(&path_add, cx);
                                                }))
                                                .child("Keep working tree"),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(theme::font_micro()))
                                        .text_color(theme::text_dim())
                                        .child("Keep working tree stages the file as it is."),
                                ),
                        )
                        .into_any_element()
                } else {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::text_muted())
                        .child("Reading the conflict…")
                        .into_any_element()
                })
                .into_any_element();
        };
        let seed = {
            let editor = self.merge_editor.as_mut().expect("the path came from it");
            if editor.choices.len() != file.blocks.len() {
                editor.choices.resize(file.blocks.len(), None);
                editor.manual_blocks.resize(file.blocks.len(), false);
                editor.current = editor.current.min(editor.choices.len().saturating_sub(1));
            }
            if editor.ready {
                false
            } else {
                editor.ready = true;
                editor.diff = merge_diff::align(
                    &file.local,
                    file.base.as_deref().unwrap_or_default(),
                    &file.incoming,
                );
                editor.result = file.worktree.clone();
                editor.result_ranges = file
                    .blocks
                    .iter()
                    .map(|block| block.range.clone())
                    .collect();
                if editor.result_input.is_none() {
                    let input = cx.new(|cx| {
                        TextareaState::new(window, cx).default_value(editor.result.clone())
                    });
                    let subscription =
                        cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
                            match event {
                                InputEvent::Change => {
                                    let value = input.read(cx).value().to_string();
                                    this.on_result_edited(value, cx);
                                }
                                // the shared Input binding turns ctrl-enter into
                                // a silent newline plus this event; the silent
                                // insert never reaches `result`, so accepting
                                // here writes the text the user saw before it
                                InputEvent::PressEnter {
                                    secondary: true, ..
                                } => {
                                    // the input inserted the ctrl-enter newline
                                    // before this event; accepting the text
                                    // with it would add a blank line
                                    if let Some(editor) = this.merge_editor.as_mut()
                                        && is_single_newline_insert(
                                            &editor.last_change_before,
                                            &editor.result,
                                        )
                                    {
                                        editor.result = editor.last_change_before.clone();
                                    }
                                    this.accept_merge(cx);
                                }
                                _ => {}
                            }
                        });
                    editor.result_input = Some(input);
                    editor.result_subscription = Some(subscription);
                }
                !editor.scrolled
            }
        };
        if seed {
            if let Some(editor) = self.merge_editor.as_mut() {
                editor.scrolled = true;
            }
            self.scroll_to_current(cx);
        }

        let (choices, current, manual_blocks, ready, result_accept) = {
            let editor = self.merge_editor.as_ref().expect("checked");
            (
                editor.choices.clone(),
                editor.current,
                editor.manual_blocks.clone(),
                editor.ready,
                !holds_conflict_markers(&editor.result),
            )
        };
        let total = file.blocks.len();
        let resolved = if total == 0 {
            usize::from(
                self.merge_editor
                    .as_ref()
                    .is_some_and(|editor| editor.whole.is_some()),
            )
        } else {
            (0..total)
                .filter(|index| {
                    choices.get(*index).copied().flatten().is_some()
                        || manual_blocks.get(*index).copied().unwrap_or(false)
                })
                .count()
        };
        let accept = if total == 0 {
            self.merge_editor
                .as_ref()
                .is_some_and(|editor| editor.whole.is_some())
        } else {
            ready && result_accept
        };
        let whole = self.merge_editor.as_ref().and_then(|editor| editor.whole);
        let current = if total == 0 {
            0
        } else {
            current.min(total - 1)
        };
        let local_chosen = if total == 0 {
            whole == Some(ConflictSide::Local)
        } else {
            choices.get(current).copied().flatten() == Some(ConflictPick::Local)
        };
        let incoming_chosen = if total == 0 {
            whole == Some(ConflictSide::Incoming)
        } else {
            choices.get(current).copied().flatten() == Some(ConflictPick::Incoming)
        };
        let layout = self
            .merge_editor
            .as_ref()
            .map(|editor| editor.layout)
            .unwrap_or(MergeLayout::Split);
        let show_base = self
            .merge_editor
            .as_ref()
            .is_some_and(|editor| editor.show_base);
        let base_available =
            !file.blocks.is_empty() && file.blocks.iter().all(|block| block.base.is_some());
        let columns_available = self.store.read(cx).pane_width > 1000.;
        let head = self.render_merge_head(
            MergeHead {
                path: path.clone(),
                resolved,
                total,
                accept,
                layout,
                columns_available,
                base_available,
                show_base,
            },
            cx,
        );

        // the numbered strip: a press jumps to a block
        let mut strip = div()
            .id("merge-strip")
            .debug_selector(|| "merge-strip".into())
            .flex_shrink_0()
            .h(px(40.))
            .px(px(warpui_kit_core::shell::DETAIL_PADDING))
            .py_1()
            .border_b_1()
            .border_color(theme::hairline())
            .flex()
            .flex_row()
            .items_center()
            .gap_1();
        for index in 0..total {
            let chosen = choices.get(index).copied().flatten();
            let is_current = index == current;
            let mut chip = div()
                .id(SharedString::from(format!("merge-block-{index}")))
                .debug_selector(move || format!("merge-block-{index}"))
                .a11y_button(format!("Conflict {}", index + 1))
                .cursor_pointer()
                .h(px(24.))
                .min_w(px(24.))
                .px_1()
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::font_meta()))
                .text_color(if chosen.is_some() {
                    theme::text()
                } else {
                    theme::text_dim()
                })
                .when(is_current, |chip| {
                    chip.border_1().border_color(theme::accent())
                })
                .when(!is_current, |chip| chip.bg(theme::well()))
                .on_click(cx.listener(move |this, _, _, cx| this.set_current(index, cx)));
            match chosen {
                Some(ConflictPick::Local) => {
                    chip = chip.child(icon(
                        "icons/icon-check.svg",
                        px(theme::icon_tiny()),
                        theme::SUCCESS_BRIGHT,
                    ));
                }
                Some(ConflictPick::Incoming) => {
                    chip = chip.child(icon(
                        "icons/icon-arrow-down.svg",
                        px(theme::icon_tiny()),
                        theme::accent(),
                    ));
                }
                Some(ConflictPick::LocalThenIncoming | ConflictPick::IncomingThenLocal) => {
                    chip = chip.child("±");
                }
                None if manual_blocks.get(index).copied().unwrap_or(false) => {
                    chip = chip.child(icon(
                        "icons/icon-check.svg",
                        px(theme::icon_tiny()),
                        theme::text_muted(),
                    ));
                }
                None => chip = chip.child(format!("{}", index + 1)),
            }
            strip = strip.child(chip);
        }

        let font_detail = theme::font_mono_meta();
        let rows = self.merge_editor.as_ref().expect("checked").diff.clone();
        let diff_scroll = self
            .merge_editor
            .as_ref()
            .expect("checked")
            .diff_scroll
            .clone();
        let take_view = cx.entity();
        let on_take: Rc<BlockTake> = Rc::new(move |index, side, window, cx: &mut gpui::App| {
            take_view.update(cx, |this, cx| {
                this.choose_block(index, side, window, cx);
            });
        });
        let sides = merge_diff_view(
            MergeDiffView {
                file: &file,
                rows: &rows,
                current,
                show_base: show_base && base_available,
                local_chosen,
                incoming_chosen,
                font_detail,
                scroll: &diff_scroll,
                on_local: if total == 0 {
                    Some(Box::new(cx.listener(|this, _, window, cx| {
                        this.choose_side(ConflictSide::Local, window, cx)
                    })))
                } else {
                    None
                },
                on_incoming: if total == 0 {
                    Some(Box::new(cx.listener(|this, _, window, cx| {
                        this.choose_side(ConflictSide::Incoming, window, cx)
                    })))
                } else {
                    None
                },
                choices: &choices,
                on_take,
            },
            window,
        );

        let (result_input, preview) = self
            .merge_editor
            .as_ref()
            .map(|editor| (editor.result_input.clone(), editor.preview))
            .unwrap_or_default();
        let preview_result: gpui::AnyElement = match self.merge_editor.as_ref() {
            Some(editor) if preview => merge_result_preview(
                &editor.result,
                &editor.result_ranges,
                &editor.choices,
                &editor.manual_blocks,
            ),
            _ => div().into_any_element(),
        };
        let result: gpui::AnyElement = if preview && result_input.is_some() {
            preview_result
        } else {
            match result_input {
                Some(input) => div()
                    .id("merge-result-scroll")
                    .debug_selector(|| "merge-result-scroll".into())
                    .flex_1()
                    .min_h_0()
                    .p_2()
                    .child(
                        // a pane-sized editor, not a form field: the area's own
                        // h_auto comes back as a one-row strip here, so pin it to
                        // the well and let the editor scroll inside (the commit
                        // box's pattern)
                        warpui_kit_core::field::mono_area("merge-result", &input, window, cx)
                            .h_full()
                            .min_h(px(0.)),
                    )
                    .into_any_element(),
                None => div().into_any_element(),
            }
        };
        let result_pane = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::well())
            .border_t_1()
            .border_color(theme::hairline())
            .child(
                div()
                    .flex_shrink_0()
                    .h(px(30.))
                    .px_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::hairline())
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text_dim())
                            .child("Result"),
                    )
                    .child(
                        div()
                            .id("merge-preview-toggle")
                            .debug_selector(|| "merge-preview-toggle".into())
                            .a11y_button(if preview {
                                "Edit the result"
                            } else {
                                "Preview the result"
                            })
                            .cursor_pointer()
                            .h(px(22.))
                            .px_2()
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::text_muted())
                            .hover(|style| style.bg(theme::raised()).text_color(theme::text()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(editor) = this.merge_editor.as_mut() {
                                    editor.preview = !editor.preview;
                                }
                                cx.notify();
                            }))
                            .child(if preview { "Edit" } else { "Preview" }),
                    ),
            )
            .child(result);

        let body: gpui::AnyElement = if columns_available && layout == MergeLayout::Columns {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(sides)
                .child(result_pane)
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(div().flex_1().min_h_0().flex().child(sides))
                .child(div().flex_1().min_h_0().flex().child(result_pane))
                .into_any_element()
        };

        div()
            .id("merge-editor")
            .debug_selector(|| "merge-editor".into())
            .track_focus(&self.editor_focus)
            .on_key_down(cx.listener(|this, event, window, cx| {
                if this.handle_merge_editor_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(head)
            .children((total > 0).then_some(strip))
            .child(body)
            .into_any_element()
    }

    /// The editor's head: Back, path, `n of m`, prev/next, take-alls, Accept,
    /// and the manual toggle.
    fn render_merge_head(&self, head: MergeHead, cx: &mut Context<Self>) -> gpui::AnyElement {
        let MergeHead {
            path,
            resolved,
            total,
            accept,
            layout,
            columns_available,
            base_available,
            show_base,
        } = head;
        let mut head = div()
            .id("merge-head")
            .debug_selector(|| "merge-head".into())
            .flex_shrink_0()
            .h(px(TOOLBAR_H))
            .px(px(warpui_kit_core::shell::DETAIL_PADDING))
            .border_b_1()
            .border_color(theme::hairline())
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("merge-back")
                    .debug_selector(|| "merge-back".into())
                    .a11y_button("Back to the file list")
                    .cursor_pointer()
                    .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
                    .px_2()
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::font_control()))
                    .text_color(theme::text_muted())
                    .hover(|style| style.bg(theme::raised()).text_color(theme::text()))
                    .on_click(cx.listener(|this, _, _, cx| this.close_merge_editor(cx)))
                    .child(icon(
                        "icons/icon-arrow-left.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    ))
                    .child("Files"),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(theme::font_control()))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::text())
                    .child(path.to_string()),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_size(px(theme::font_meta()))
                    .text_color(if total > 0 && resolved == total {
                        theme::SUCCESS_BRIGHT
                    } else {
                        theme::text_dim()
                    })
                    .child(if total == 0 {
                        if resolved == 1 {
                            "Whole-file take".to_string()
                        } else {
                            "No text blocks".to_string()
                        }
                    } else {
                        format!("{resolved} of {total} resolved")
                    }),
            );
        if total > 0 {
            head = head
                .child(merge_icon_verb(
                    "merge-prev",
                    "‹",
                    "Previous conflict",
                    true,
                    cx.listener(|this, _, _, cx| this.step_conflict(-1, cx)),
                ))
                .child(merge_icon_verb(
                    "merge-next",
                    "›",
                    "Next conflict",
                    true,
                    cx.listener(|this, _, _, cx| this.step_conflict(1, cx)),
                ));
        }
        head = head
            .child(merge_verb(
                "merge-take-local",
                "Take all Local",
                total > 0,
                cx.listener(|this, _, window, cx| this.take_all(ConflictSide::Local, window, cx)),
            ))
            .child(merge_verb(
                "merge-take-incoming",
                "Take all Incoming",
                total > 0,
                cx.listener(|this, _, window, cx| {
                    this.take_all(ConflictSide::Incoming, window, cx)
                }),
            ))
            .when(total > 0, |head| {
                head.child(merge_verb(
                    "merge-both-local-first",
                    "L+I",
                    true,
                    cx.listener(|this, _, window, cx| this.take_both(false, window, cx)),
                ))
                .child(merge_verb(
                    "merge-both-incoming-first",
                    "I+L",
                    true,
                    cx.listener(|this, _, window, cx| this.take_both(true, window, cx)),
                ))
            })
            .child(merge_verb(
                "merge-layout",
                if layout == MergeLayout::Columns {
                    "Split"
                } else {
                    "Columns"
                },
                columns_available,
                cx.listener(|this, _, _, cx| this.toggle_layout(cx)),
            ))
            .when(base_available, |head| {
                head.child(merge_verb(
                    "merge-base-toggle",
                    if show_base { "Hide base" } else { "Base" },
                    true,
                    cx.listener(|this, _, _, cx| this.toggle_base(cx)),
                ))
            })
            .child(merge_verb(
                "merge-reset",
                "Reset",
                total > 0,
                cx.listener(|this, _, window, cx| this.reset_result(window, cx)),
            ))
            .child(crate::ui::primary_button(
                "merge-accept",
                "Accept Merge",
                accept,
                cx.listener(|this, _, _, cx| this.accept_merge(cx)),
            ));
        head.into_any_element()
    }
}

/// A bordered text verb in the editor's head (take alls, manual toggle).
fn merge_verb(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .flex_shrink_0()
        .h(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .px_3()
        .rounded(px(6.))
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::panel())
        .flex()
        .items_center()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .text_color(theme::text())
                .hover(|style| style.bg(theme::raised()))
                .on_click(on_click)
        })
        .when(!enabled, |button| {
            button.opacity(0.45).text_color(theme::text_dim())
        })
        .child(label)
        .into_any_element()
}

/// A square glyph verb (prev/next) in the editor's head.
fn merge_icon_verb(
    id: &'static str,
    glyph: &'static str,
    label: &'static str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .a11y_button(label)
        .flex_shrink_0()
        .size(px(warpui_kit_core::shell::CONTROL_HEIGHT))
        .rounded(px(6.))
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::panel())
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_control()))
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .text_color(theme::text())
                .hover(|style| style.bg(theme::raised()))
                .on_click(on_click)
        })
        .when(!enabled, |button| {
            button.opacity(0.45).text_color(theme::text_dim())
        })
        .child(glyph)
        .into_any_element()
}

/// The smallest span that turns `old` into `new`: (start, old end, new end),
/// on char boundaries.
fn minimal_edit(old: &str, new: &str) -> (usize, usize, usize) {
    let max = old.len().min(new.len());
    let mut prefix = 0;
    while prefix < max && old.as_bytes()[prefix] == new.as_bytes()[prefix] {
        prefix += 1;
    }
    while prefix > 0 && (!old.is_char_boundary(prefix) || !new.is_char_boundary(prefix)) {
        prefix -= 1;
    }
    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && old.as_bytes()[old.len() - 1 - suffix] == new.as_bytes()[new.len() - 1 - suffix]
    {
        suffix += 1;
    }
    while suffix > 0
        && (!old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix))
    {
        suffix -= 1;
    }
    (prefix, old.len() - suffix, new.len() - suffix)
}

/// Maps a block range through one edit. A range before the edit stays, one
/// after shifts by the length delta, and one the edit touches grows to cover
/// the edited span.
fn map_range(range: &mut std::ops::Range<usize>, start: usize, old_end: usize, new_end: usize) {
    let delta = new_end as isize - old_end as isize;
    if range.end <= start {
        return;
    }
    if range.start >= old_end {
        range.start = range.start.saturating_add_signed(delta);
        range.end = range.end.saturating_add_signed(delta);
        return;
    }
    let new_start = range.start.min(start);
    let new_end_pos = if range.end >= old_end {
        range.end.saturating_add_signed(delta)
    } else {
        new_end
    };
    *range = new_start..new_end_pos.max(new_start);
}

/// True while a conflict marker line is left: the gate on Accept.
fn holds_conflict_markers(text: &str) -> bool {
    text.lines().any(|line| {
        line.starts_with("<<<<<<<") || line.starts_with(">>>>>>>") || line.starts_with("|||||||")
    })
}

/// The changed middle of two corresponding lines, char-boundary safe; `None`
/// when the lines are equal.
fn changed_span(a: &str, b: &str) -> Option<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut prefix = 0;
    while prefix < a_chars.len() && prefix < b_chars.len() && a_chars[prefix] == b_chars[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < a_chars.len() - prefix
        && suffix < b_chars.len() - prefix
        && a_chars[a_chars.len() - 1 - suffix] == b_chars[b_chars.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let byte_span = |chars: &[char], start: usize, end: usize| -> std::ops::Range<usize> {
        let start_byte: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
        let end_byte: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();
        start_byte..end_byte
    };
    let a_range = byte_span(&a_chars, prefix, a_chars.len() - suffix);
    let b_range = byte_span(&b_chars, prefix, b_chars.len() - suffix);
    if a_range.is_empty() && b_range.is_empty() {
        None
    } else {
        Some((a_range, b_range))
    }
}

/// Whether `after` is `before` with exactly one `\n` inserted: the shape a
/// ctrl-enter leaves in a textarea before its PressEnter event.
fn is_single_newline_insert(before: &str, after: &str) -> bool {
    if after.len() != before.len() + 1 {
        return false;
    }
    let mut prefix = 0;
    while prefix < before.len() && before.as_bytes()[prefix] == after.as_bytes()[prefix] {
        prefix += 1;
    }
    after.as_bytes().get(prefix) == Some(&b'\n') && before[prefix..] == after[prefix + 1..]
}

/// The editor head's state: the path, progress, and which verbs are live.
struct MergeHead {
    path: String,
    resolved: usize,
    total: usize,
    accept: bool,
    layout: MergeLayout,
    columns_available: bool,
    base_available: bool,
    show_base: bool,
}

/// A click handler on the editor's own controls.
type MergeClick = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App)>;

/// A block take from a diff row: the row knows which block it starts, the
/// click knows which side.
type BlockTake = dyn Fn(usize, ConflictSide, &mut Window, &mut gpui::App);

/// Which side's text a Result line came from, for the coloured preview.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ResultLineKind {
    Plain,
    Local,
    Incoming,
    Both,
    Unresolved,
    Manual,
    Marker,
}

fn is_marker_line(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("<<<<<<<")
        || line.starts_with("|||||||")
        || line.starts_with("=======")
        || line.starts_with(">>>>>>>")
}

/// The Result in its diff colours: the editable textarea is one click away.
/// Each line takes its block's colour (Local red, Incoming green, both the
/// accent), the marker lines stay dim, and an unresolved or hand-edited block
/// reads as such.
fn merge_result_preview(
    result: &str,
    ranges: &[std::ops::Range<usize>],
    choices: &[Option<ConflictPick>],
    manual: &[bool],
) -> gpui::AnyElement {
    let lines = merge_diff::display_lines(result);
    let mut kinds = vec![ResultLineKind::Plain; lines.len()];
    let mut starts = vec![0usize];
    for (index, byte) in result.bytes().enumerate() {
        if byte == b'\n' && index + 1 < result.len() {
            starts.push(index + 1);
        }
    }
    let line_of = |offset: usize| {
        starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    };
    for (index, range) in ranges.iter().enumerate() {
        if range.start >= range.end || range.end > result.len() {
            continue;
        }
        let first = line_of(range.start);
        let last = line_of(range.end.saturating_sub(1)).min(lines.len().saturating_sub(1));
        let kind = match choices.get(index).copied().flatten() {
            Some(ConflictPick::Local) => ResultLineKind::Local,
            Some(ConflictPick::Incoming) => ResultLineKind::Incoming,
            Some(ConflictPick::LocalThenIncoming | ConflictPick::IncomingThenLocal) => {
                ResultLineKind::Both
            }
            None if manual.get(index).copied().unwrap_or(false) => ResultLineKind::Manual,
            None => ResultLineKind::Unresolved,
        };
        for (line, slot) in kinds
            .iter_mut()
            .enumerate()
            .skip(first)
            .take(last - first + 1)
        {
            let text = lines.get(line).copied().unwrap_or("");
            *slot = if is_marker_line(text) {
                ResultLineKind::Marker
            } else {
                kind
            };
        }
    }
    let lines: Rc<Vec<SharedString>> = Rc::new(
        lines
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<SharedString>>(),
    );
    let kinds = Rc::new(kinds);
    let row_h = (theme::font_mono_meta() * 1.5).max(16.);
    let list = uniform_list(
        "merge-result-preview",
        lines.len(),
        move |range, _window, _cx| {
            range
                .map(|index| {
                    let kind = kinds[index];
                    let line = lines[index].clone();
                    let mut row = div()
                        .w_full()
                        .h(px(row_h))
                        .flex()
                        .items_center()
                        .font_family(theme::code_font())
                        .text_size(px(theme::font_mono_meta()))
                        .text_color(theme::text());
                    if let Some(color) = match kind {
                        ResultLineKind::Local => Some(theme::DANGER),
                        ResultLineKind::Incoming => Some(theme::SUCCESS_BRIGHT),
                        ResultLineKind::Both => Some(theme::accent()),
                        _ => None,
                    } {
                        row = row.child(div().w(px(3.)).h_full().flex_shrink_0().bg(color));
                    }
                    if let Some(bg) = match kind {
                        ResultLineKind::Local => Some(theme::tint(theme::DANGER)),
                        ResultLineKind::Incoming => Some(theme::tint(theme::SUCCESS_BRIGHT)),
                        ResultLineKind::Both => Some(theme::tint(theme::accent())),
                        ResultLineKind::Unresolved => Some(theme::tint(theme::WARN)),
                        ResultLineKind::Manual | ResultLineKind::Marker => {
                            Some(theme::tint(theme::text_dim()))
                        }
                        ResultLineKind::Plain => None,
                    } {
                        row = row.bg(bg);
                    }
                    row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .px_2()
                            .truncate()
                            .when(kind == ResultLineKind::Marker, |text| {
                                text.text_color(theme::text_dim())
                            })
                            .child(line),
                    )
                    .into_any_element()
                })
                .collect()
        },
    )
    .size_full();
    div()
        .id("merge-result-preview-pane")
        .debug_selector(|| "merge-result-preview-pane".into())
        .flex_1()
        .min_h_0()
        .child(list)
        .into_any_element()
}

/// The aligned diff: one row list over the base, two or three columns, with
/// each side's take for the current block in its head.
struct MergeDiffView<'a> {
    file: &'a GitConflictFile,
    rows: &'a [merge_diff::MergeRow],
    current: usize,
    show_base: bool,
    local_chosen: bool,
    incoming_chosen: bool,
    font_detail: f32,
    scroll: &'a UniformListScrollHandle,
    on_local: Option<MergeClick>,
    on_incoming: Option<MergeClick>,
    /// One pick per block, for the row checkboxes.
    choices: &'a [Option<ConflictPick>],
    on_take: Rc<BlockTake>,
}

/// A row's own take checkbox: shown where a conflict block starts on that
/// side, so a side can be taken without leaving the row.
struct DiffTake {
    checked: bool,
    block: usize,
    side: ConflictSide,
    on_take: Rc<BlockTake>,
}

/// One cell of a diff row: a side's line, or the hatched gap where that side
/// deleted the base line.
struct DiffCell<'a> {
    line: Option<usize>,
    lines: &'a [SharedString],
    gap: bool,
    changed: bool,
    /// The changed middle of a line the current block replaced on both sides.
    word: Option<std::ops::Range<usize>>,
    word_color: gpui::Rgba,
    change_color: gpui::Rgba,
    gutter_w: f32,
    font: f32,
    first: bool,
    /// The cell's probe selector: which row and side it is, and whether it is
    /// the side's line or the gap where that side deleted it.
    debug: SharedString,
    /// The block take at this row, when a block starts here.
    take: Option<DiffTake>,
}

fn diff_cell(cell: DiffCell<'_>, window: &mut Window) -> gpui::AnyElement {
    let DiffCell {
        line,
        lines,
        gap,
        changed,
        word,
        word_color,
        change_color,
        gutter_w,
        font,
        first,
        debug,
        take,
    } = cell;
    let mut cell = div()
        .id(debug.clone())
        .debug_selector(move || debug.to_string())
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .overflow_hidden()
        .when(!first, |cell| {
            cell.border_l_1().border_color(theme::hairline())
        });
    if gap {
        cell = cell.bg(theme::tint(theme::text_dim()));
    }
    if let Some(take) = take {
        let DiffTake {
            checked,
            block,
            side,
            on_take,
        } = take;
        let selector = format!(
            "merge-take-{block}-{}",
            if side == ConflictSide::Local {
                "local"
            } else {
                "incoming"
            }
        );
        cell = cell.child(
            div()
                .id(SharedString::from(selector.clone()))
                .debug_selector(move || selector.clone())
                .a11y_toggle(
                    format!(
                        "Take {} for block {}",
                        if side == ConflictSide::Local {
                            "Local"
                        } else {
                            "Incoming"
                        },
                        block + 1
                    ),
                    checked,
                )
                .cursor_pointer()
                .ml_1()
                .size(px(14.))
                .flex_shrink_0()
                .rounded(px(3.))
                .border_1()
                .border_color(if checked {
                    theme::accent()
                } else {
                    theme::line_strong()
                })
                .when(checked, |box_| box_.bg(theme::accent()))
                .flex()
                .items_center()
                .justify_center()
                .when(checked, |box_| {
                    box_.child(icon(
                        "icons/icon-check.svg",
                        px(theme::icon_tiny() - 2.),
                        theme::on_accent(),
                    ))
                })
                .on_click(move |_, window, cx| on_take(block, side, window, cx)),
        );
    }
    if gap {
        return cell.into_any_element();
    }
    if changed {
        cell = cell
            .bg(theme::tint(change_color))
            // the change bar a diff's gutter carries; the tint alone reads as
            // selection in this list
            .child(div().w(px(3.)).h_full().flex_shrink_0().bg(change_color));
    }
    let Some(index) = line else {
        return cell.into_any_element();
    };
    let text = lines.get(index).cloned().unwrap_or_default();
    let word = word.filter(|span| {
        span.start <= span.end
            && span.end <= text.len()
            && text.is_char_boundary(span.start)
            && text.is_char_boundary(span.end)
    });
    let mut style = window.text_style();
    style.color = theme::text().into();
    let text_element: gpui::AnyElement = match word {
        Some(span) => StyledText::new(text.to_string())
            .with_default_highlights(
                &style,
                vec![(
                    span,
                    HighlightStyle {
                        background_color: Some(theme::tint(word_color)),
                        ..Default::default()
                    },
                )],
            )
            .into_any_element(),
        None => div().min_w_0().truncate().child(text).into_any_element(),
    };
    cell.child(
        div()
            .w(px(gutter_w))
            .flex_shrink_0()
            .pr_1()
            .flex()
            .justify_end()
            .text_size(px(font * 0.9))
            .text_color(theme::text_dim())
            .child(format!("{}", index + 1)),
    )
    .child(div().flex_1().min_w_0().pl_2().child(text_element))
    .into_any_element()
}

/// One column head: the current block's take on that side, or the read-only
/// Base label. The id doubles as the probe's selector, as the panes' heads did.
fn diff_column_head(
    id: &'static str,
    label: &'static str,
    chosen: bool,
    on_toggle: Option<MergeClick>,
    first: bool,
) -> gpui::AnyElement {
    let mut head = div()
        .id(SharedString::from(id))
        .debug_selector(move || id.to_string())
        .flex_1()
        .min_w_0()
        .h_full()
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .when(!first, |head| {
            head.border_l_1().border_color(theme::hairline())
        })
        .text_size(px(theme::font_meta()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::text_dim());
    if let Some(on_toggle) = on_toggle {
        head = head
            .child(
                div()
                    .id(SharedString::from(format!("{id}-take")))
                    .debug_selector(move || format!("{id}-take"))
                    .a11y_toggle(format!("Take {label}"), chosen)
                    .cursor_pointer()
                    .size(px(18.))
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if chosen {
                        theme::accent()
                    } else {
                        theme::line_strong()
                    })
                    .when(chosen, |box_| box_.bg(theme::accent()))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(chosen, |box_| {
                        box_.child(icon(
                            "icons/icon-check.svg",
                            px(theme::icon_tiny()),
                            theme::on_accent(),
                        ))
                    })
                    .on_click(on_toggle),
            )
            .child(label);
    } else {
        head = head.child(label);
    }
    head.into_any_element()
}

/// The editor's whole-file diff: one virtualised list of base-aligned rows,
/// the current block's word changes, and the two sides' takes. The rows come
/// from [`merge_diff::align`]; a cell with no line is a gap, so deleted lines
/// read as holes instead of shifting the other side's lines.
fn merge_diff_view(view: MergeDiffView<'_>, _window: &mut Window) -> gpui::AnyElement {
    let MergeDiffView {
        file,
        rows,
        current,
        show_base,
        local_chosen,
        incoming_chosen,
        font_detail,
        scroll,
        on_local,
        on_incoming,
        choices,
        on_take,
    } = view;
    let choices = Rc::new(choices.to_vec());
    let local_lines: Rc<Vec<SharedString>> = Rc::new(
        merge_diff::display_lines(&file.local)
            .into_iter()
            .map(SharedString::from)
            .collect(),
    );
    let incoming_lines: Rc<Vec<SharedString>> = Rc::new(
        merge_diff::display_lines(&file.incoming)
            .into_iter()
            .map(SharedString::from)
            .collect(),
    );
    let base_lines: Rc<Vec<SharedString>> = Rc::new(
        file.base
            .as_deref()
            .map(merge_diff::display_lines)
            .unwrap_or_default()
            .into_iter()
            .map(SharedString::from)
            .collect(),
    );
    let local_blocks: Rc<Vec<std::ops::Range<usize>>> = Rc::new(
        file.blocks
            .iter()
            .map(|block| block.local_lines.clone())
            .collect(),
    );
    let incoming_blocks: Rc<Vec<std::ops::Range<usize>>> = Rc::new(
        file.blocks
            .iter()
            .map(|block| block.incoming_lines.clone())
            .collect(),
    );
    let current_block = file.blocks.get(current);
    let current_local = current_block.map(|block| block.local_lines.clone());
    let current_incoming = current_block.map(|block| block.incoming_lines.clone());
    let block_line_map = |ranges: &[std::ops::Range<usize>], count: usize| {
        let mut map = vec![None; count];
        for (index, range) in ranges.iter().enumerate() {
            map[range.start..range.end.min(count)].fill(Some(index));
        }
        map
    };
    let local_of_line = block_line_map(&local_blocks, local_lines.len());
    let incoming_of_line = block_line_map(&incoming_blocks, incoming_lines.len());
    let base_ranges: Vec<std::ops::Range<usize>> = file
        .blocks
        .iter()
        .map(|block| block.base_lines.clone())
        .collect();
    let base_of_line = block_line_map(&base_ranges, base_lines.len());
    // one checkbox per block, on the block's first row and on both sides (a
    // deleted side's box sits in its gap)
    let mut row_blocks = vec![None; rows.len()];
    let mut seen = vec![false; file.blocks.len()];
    for (row_index, row) in rows.iter().enumerate() {
        let block = row
            .local
            .and_then(|line| local_of_line.get(line).copied().flatten())
            .or_else(|| {
                row.incoming
                    .and_then(|line| incoming_of_line.get(line).copied().flatten())
            })
            .or_else(|| {
                row.base
                    .and_then(|line| base_of_line.get(line).copied().flatten())
            });
        if let Some(block) = block
            && !seen[block]
        {
            seen[block] = true;
            row_blocks[row_index] = Some(block);
        }
    }
    let rows = Rc::new(rows.to_vec());
    let max_lines = local_lines
        .len()
        .max(incoming_lines.len())
        .max(base_lines.len());
    let gutter_w = max_lines.to_string().len().max(2) as f32 * 7.5 + 10.;
    let row_h = (font_detail * 1.5).max(16.);
    let row_font = font_detail;

    let list = uniform_list("merge-diff-file", rows.len(), move |range, window, _cx| {
        range
            .map(|index| {
                let row = rows[index];
                let local_current = current_local
                    .as_ref()
                    .is_some_and(|block| row.local.is_some_and(|line| block.contains(&line)))
                    && row.local.is_some();
                let incoming_current = current_incoming
                    .as_ref()
                    .is_some_and(|block| row.incoming.is_some_and(|line| block.contains(&line)))
                    && row.incoming.is_some();
                let local_in_block = local_blocks
                    .iter()
                    .any(|block| row.local.is_some_and(|line| block.contains(&line)));
                let incoming_in_block = incoming_blocks
                    .iter()
                    .any(|block| row.incoming.is_some_and(|line| block.contains(&line)));
                let mut line = div()
                    .w_full()
                    .h(px(row_h))
                    .flex()
                    .flex_row()
                    .font_family(theme::code_font())
                    .text_size(px(row_font))
                    .text_color(theme::text());
                if local_current || incoming_current {
                    line = line.bg(theme::tint(theme::accent()));
                } else if local_in_block || incoming_in_block {
                    line = line.bg(theme::raised());
                }

                let local_text = row.local.map(|line| &local_lines[line]);
                let base_text = row.base.map(|line| &base_lines[line]);
                let incoming_text = row.incoming.map(|line| &incoming_lines[line]);
                let local_changed = match (local_text, base_text) {
                    (Some(local), Some(base)) => local != base,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                let incoming_changed = match (incoming_text, base_text) {
                    (Some(incoming), Some(base)) => incoming != base,
                    (Some(_), None) => true,
                    (None, _) => false,
                };
                let (local_word, incoming_word) = if local_current && incoming_current {
                    match (local_text, incoming_text) {
                        (Some(local), Some(incoming)) => match changed_span(local, incoming) {
                            Some((local, incoming)) => (Some(local), Some(incoming)),
                            None => (None, None),
                        },
                        _ => (None, None),
                    }
                } else {
                    (None, None)
                };

                let row_block = row_blocks[index];
                let take_for = |side: ConflictSide| {
                    row_block.map(|block| DiffTake {
                        checked: matches!(
                            (choices.get(block).copied().flatten(), side),
                            (Some(ConflictPick::Local), ConflictSide::Local)
                                | (Some(ConflictPick::Incoming), ConflictSide::Incoming)
                                | (
                                    Some(
                                        ConflictPick::LocalThenIncoming
                                            | ConflictPick::IncomingThenLocal
                                    ),
                                    _
                                )
                        ),
                        block,
                        side,
                        on_take: on_take.clone(),
                    })
                };
                let cell_selector = |side: &str, line: Option<usize>| {
                    SharedString::from(format!(
                        "merge-diff-{side}-{index}-{}",
                        line.map_or_else(|| "gap".to_string(), |line| (line + 1).to_string())
                    ))
                };
                line = line.child(diff_cell(
                    DiffCell {
                        line: row.local,
                        lines: &local_lines,
                        gap: row.local.is_none(),
                        changed: local_changed,
                        word: local_word,
                        word_color: theme::DANGER,
                        change_color: theme::DANGER,
                        gutter_w,
                        font: row_font,
                        first: true,
                        debug: cell_selector("local", row.local),
                        take: take_for(ConflictSide::Local),
                    },
                    window,
                ));
                if show_base {
                    line = line.child(diff_cell(
                        DiffCell {
                            line: row.base,
                            lines: &base_lines,
                            gap: row.base.is_none(),
                            changed: false,
                            word: None,
                            word_color: theme::text_dim(),
                            change_color: theme::text_dim(),
                            gutter_w,
                            font: row_font,
                            first: false,
                            debug: cell_selector("base", row.base),
                            take: None,
                        },
                        window,
                    ));
                }
                line = line.child(diff_cell(
                    DiffCell {
                        line: row.incoming,
                        lines: &incoming_lines,
                        gap: row.incoming.is_none(),
                        changed: incoming_changed,
                        word: incoming_word,
                        word_color: theme::SUCCESS_BRIGHT,
                        change_color: theme::SUCCESS_BRIGHT,
                        gutter_w,
                        font: row_font,
                        first: false,
                        debug: cell_selector("incoming", row.incoming),
                        take: take_for(ConflictSide::Incoming),
                    },
                    window,
                ));
                line
            })
            .collect()
    })
    .track_scroll(scroll)
    .size_full();

    let mut head = div()
        .flex_shrink_0()
        .h(px(34.))
        .flex()
        .flex_row()
        .border_b_1()
        .border_color(theme::hairline());
    head = head.child(diff_column_head(
        "merge-local",
        "Local",
        local_chosen,
        on_local,
        true,
    ));
    if show_base {
        head = head.child(diff_column_head("merge-base", "Base", false, None, false));
    }
    head = head.child(diff_column_head(
        "merge-incoming",
        "Incoming",
        incoming_chosen,
        on_incoming,
        false,
    ));

    div()
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .flex_col()
        .bg(theme::well())
        .border_r_1()
        .border_color(theme::hairline())
        .child(head)
        .child(div().flex_1().min_h_0().child(list))
        .into_any_element()
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

/// The footer a stopped merge shows instead of the commit card: the band's
/// Continue is the one way to finish, so a second message box would fork it.
fn merge_commit_hint() -> gpui::AnyElement {
    div()
        .debug_selector(|| "merge-commit-hint".into())
        .flex_shrink_0()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(theme::hairline())
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_muted())
        .child("Resolve the files above, then Continue in the merge band")
        .into_any_element()
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

/// The conflicted row's context menu: which path and where it opened.
#[derive(Clone)]
struct FileMenu {
    path: String,
    at: gpui::Point<gpui::Pixels>,
}

/// The merge editor's view state; the conflicted file itself is read from the
/// store when the net thread answers.
struct MergeEditor {
    path: String,
    /// One pick per marker block; `None` is unresolved (or edited by hand).
    choices: Vec<Option<ConflictPick>>,
    current: usize,
    /// The whole-file choice for a conflict with no marker blocks.
    whole: Option<ConflictSide>,
    /// Side-by-side sides over the result, or three/four columns in one row.
    layout: MergeLayout,
    /// The optional Base column; only offered when every block has a base.
    show_base: bool,
    /// The base-aligned rows the one diff list draws.
    diff: Vec<merge_diff::MergeRow>,
    diff_scroll: UniformListScrollHandle,
    /// The first block was scrolled to once the file landed; later renders
    /// leave the reader's scroll alone.
    scrolled: bool,
    /// The editable Result: the marker-bearing worktree text, with each take
    /// and each keystroke applied in place.
    result: String,
    /// Each block's byte range in `result`, parallel to the blocks.
    result_ranges: Vec<std::ops::Range<usize>>,
    /// The editable area behind `result`; `None` until the file lands.
    result_input: Option<Entity<TextareaState>>,
    /// Kept so the area's change handler lives as long as the editor.
    result_subscription: Option<Subscription>,
    /// Blocks the user edited directly; a take clears the flag.
    manual_blocks: Vec<bool>,
    /// The Result value before the last input change; a ctrl-enter's inserted
    /// newline is recognised against it and removed before Accept.
    last_change_before: String,
    /// The Result shows the coloured read-only composition instead of the
    /// editable textarea.
    preview: bool,
    /// The file's worktree text and ranges were seeded.
    ready: bool,
}

/// How the editor's panes arrange: the default split (sides above the result)
/// or one row of columns on a wide pane.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MergeLayout {
    Split,
    Columns,
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
            // the row's verbs (merge, checkout, copy, delete) live in the
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

#[cfg(test)]
mod merge_edit_tests {
    use super::*;

    #[test]
    fn edits_before_a_block_shift_it() {
        let (start, old_end, new_end) = minimal_edit("abcXYZdef", "abcZZdef");
        assert_eq!((start, old_end, new_end), (3, 5, 4));
        let mut before = 0..3;
        map_range(&mut before, start, old_end, new_end);
        assert_eq!(before, 0..3);
        let mut after = 6..9;
        map_range(&mut after, start, old_end, new_end);
        assert_eq!(after, 5..8);
    }

    #[test]
    fn an_edit_inside_a_block_grows_its_range() {
        let (start, old_end, new_end) = minimal_edit("abcdef", "abcQQef");
        assert_eq!((start, old_end, new_end), (3, 4, 5));
        let mut range = 2..6;
        map_range(&mut range, start, old_end, new_end);
        assert_eq!(range, 2..7);
    }

    #[test]
    fn marker_lines_gate_accept() {
        assert!(holds_conflict_markers("a\n<<<<<<< HEAD\nb\n"));
        assert!(holds_conflict_markers(">>>>>>> feat\n"));
        assert!(!holds_conflict_markers("resolved\n"));
    }

    #[test]
    fn changed_span_marks_only_the_middle() {
        let (a, b) = changed_span("let x = 1;", "let y = 1;").expect("lines differ");
        assert_eq!(&"let x = 1;"[a], "x");
        assert_eq!(&"let y = 1;"[b], "y");
        assert!(changed_span("same", "same").is_none());
    }
}
