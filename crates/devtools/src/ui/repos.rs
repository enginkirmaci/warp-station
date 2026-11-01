//! The Repos page: toolbar (tag chips, refresh, totals) and the repos table.
//! The table delegate reads the store's projection at render time, so
//! filter/sort/status changes land with the next frame — no mirrored
//! row list to keep in sync. Sorting is driven entirely by the table's
//! sortable headers.

use crate::store::projection::ProjectedRow;
use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::A11y;
use crate::ui::dialogs::{AddRepoDialog, CloneDialog, Dialog};
use crate::ui::icon;
use crate::ui::inspector::RepoMenu;
use devtools_core::repo_store::RepoEntry;
use devtools_core::settings::{AppSettings, RepoSortMode};
use gpui::prelude::*;
use gpui::{App, Context, Entity, FontWeight, MouseButton, Pixels, SharedString, Window, div, px};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::table::{
    Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState,
};
use gpui_kit::component::{Sizable as _, Size};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The pane head's running totals over the live (non-archived) list.
#[derive(Default)]
struct ListStats {
    total: usize,
    /// Repos with uncommitted work, an open issue, or commits to push.
    attention: usize,
    favorites: usize,
    changes: u32,
    issues: u32,
}

/// Everything a table frame needs that depends on width, settings or the
/// projection: computed once per layout key change and shared with the
/// delegate, so per-cell work stays O(1) instead of rescanning all repos.
struct TableLayout {
    /// The inputs this layout was derived from; a mismatch means stale.
    key: LayoutKey,
    cols: Vec<ColKind>,
    metrics: LayoutMetrics,
    name: Pixels,
    settings: Rc<AppSettings>,
}

/// The inputs [`TableLayout`] is derived from; a change triggers one table
/// refresh and one layout rebuild.
#[derive(Clone, Copy, PartialEq)]
struct LayoutKey {
    width_bits: u64,
    settings_version: u64,
    widest_branch: usize,
    sort_mode: u8,
    sort_ascending: bool,
}

impl LayoutKey {
    fn of(store: &Store, available: Pixels) -> Self {
        Self {
            width_bits: f32::from(available).to_bits() as u64,
            settings_version: store.settings_version(),
            widest_branch: store.widest_branch_chars,
            sort_mode: store.sort_mode as u8,
            sort_ascending: store.sort_ascending,
        }
    }
}

pub struct ReposPage {
    pub store: Entity<Store>,
    table: Entity<TableState<ReposDelegate>>,
    pub dialog: Option<Dialog>,
    /// The table's available width, written by render each frame; the
    /// delegate distributes it across the columns (the kit's table has no
    /// flex columns — its widths only re-prepare on `TableState::refresh`).
    available_width: Rc<Cell<Pixels>>,
    /// The last layout key; a change triggers one refresh.
    layout_fingerprint: Cell<LayoutKey>,
    /// The delegate's cached layout, rebuilt by render on key changes (see
    /// [`TableLayout`]).
    table_layout: Rc<RefCell<Option<Rc<TableLayout>>>>,
    /// The rail's inline tag editor: the flag is set by the "+ tag" chip and
    /// the input is created on the next frame, where the window is in hand.
    tag_editing: bool,
    tag_draft: Option<Entity<InputState>>,
    /// The rail header's "⋯" menu while it is open.
    repo_menu: Option<RepoMenu>,
    /// The list head's sort menu.
    sort_menu_open: bool,
}

impl ReposPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store_for_table = store.clone();
        let store_for_observe = store.clone();
        // `TableState` queues its SelectRow event for a later effect flush, so
        // a boolean "currently syncing" flag would already be false by the
        // time the event arrives. Count the rows we push in from the store
        // and let the subscription swallow exactly those events.
        let suppressed_select_rows = Rc::new(Cell::new(0usize));
        let available_width = Rc::new(Cell::new(px(1280.)));
        let table_layout: Rc<RefCell<Option<Rc<TableLayout>>>> = Rc::new(RefCell::new(None));
        let table = cx.new(|cx| {
            let mut delegate = ReposDelegate::new(store.clone(), px(1280.));
            // the page feeds its precomputed layout through the shared cell
            delegate.layout = table_layout.clone();
            delegate.available_width = available_width.clone();
            TableState::new(delegate, window, cx).row_selectable(true)
        });

        let table_for_store = table.clone();
        let suppressed_for_table = suppressed_select_rows.clone();
        cx.subscribe(&table, move |_, _, event: &TableEvent, cx| {
            if let TableEvent::SelectRow(row_ix) = event {
                if suppressed_for_table.get() > 0 {
                    suppressed_for_table.set(suppressed_for_table.get() - 1);
                    return;
                }
                // group header rows are inert: never selected, never open
                // the bar (the kit reverts the transient highlight through
                // clear_selection, which emits no SelectRow)
                if table_for_store
                    .read(cx)
                    .delegate()
                    .row_is_group(*row_ix, cx)
                {
                    table_for_store.update(cx, |table, cx| table.clear_selection(cx));
                    return;
                }
                // archived rows are inert: never selected, never open the
                // bar. clear_selection emits ClearSelection (not SelectRow),
                // so reverting the table's transient highlight cannot loop.
                if table_for_store
                    .read(cx)
                    .delegate()
                    .repo_at(*row_ix, cx)
                    .is_some_and(|repo| repo.is_archived())
                {
                    table_for_store.update(cx, |table, cx| table.clear_selection(cx));
                    return;
                }
                let folder = table_for_store.read(cx).delegate().folder_at(*row_ix, cx);
                tracing::info!("row {row_ix} selected -> {folder:?}");
                store_for_table.update(cx, |s, cx| s.select_repo(folder, cx));
            }
        })
        .detach();

        // Mirror selection made anywhere else (global search, notes button,
        // E2E hooks, close_bar) into the table's highlight. The table only
        // paints the store's selection this way — it has no external binding.
        // The row scan is O(n): skip it unless the projection or the
        // selection actually moved (warp frames, toasts and log lines notify
        // constantly).
        enum Sync {
            Skip,
            Row(Option<usize>),
        }
        let table_for_observe = table.clone();
        let suppressed_for_observe = suppressed_select_rows.clone();
        let last_sync: Rc<RefCell<(u64, String)>> = Rc::new(RefCell::new((0, String::new())));
        cx.observe(&store_for_observe, move |_, store, cx| {
            let wanted = {
                let store = store.read(cx);
                let key = (
                    store.projection_generation(),
                    store.selected_repo.clone().unwrap_or_default(),
                );
                if key == *last_sync.borrow() {
                    Sync::Skip
                } else {
                    *last_sync.borrow_mut() = key.clone();
                    Sync::Row(store.selected_repo.as_deref().and_then(|folder| {
                        table_for_observe
                            .read(cx)
                            .delegate()
                            .row_for_folder(folder, cx)
                    }))
                }
            };
            if let Sync::Row(wanted) = wanted
                && wanted != table_for_observe.read(cx).selected_row()
            {
                match wanted {
                    Some(row_ix) => {
                        // the set_selected_row event is skipped by the
                        // subscription above, so select_repo never toggles an
                        // externally-selected row closed
                        suppressed_for_observe.set(suppressed_for_observe.get() + 1);
                        table_for_observe
                            .update(cx, |table, cx| table.set_selected_row(row_ix, cx));
                    }
                    None => table_for_observe.update(cx, |table, cx| table.clear_selection(cx)),
                }
            }
            cx.notify();
        })
        .detach();
        cx.observe(&store_for_observe, |this, store, cx| {
            keep_row_selected(&store, cx);
            // the menu belongs to the repo it opened on
            let selected = store.read(cx).selected_repo.clone();
            if this
                .repo_menu
                .as_ref()
                .is_some_and(|menu| Some(&menu.folder) != selected.as_ref())
            {
                this.close_repo_menu(cx);
            }
        })
        .detach();
        keep_row_selected(&store_for_observe, cx);
        Self {
            store,
            table,
            dialog: None,
            available_width,
            layout_fingerprint: Cell::new(LayoutKey {
                width_bits: 0,
                settings_version: 0,
                widest_branch: 0,
                sort_mode: 0,
                sort_ascending: false,
            }),
            table_layout,
            tag_editing: false,
            tag_draft: None,
            repo_menu: None,
            sort_menu_open: false,
        }
    }
}

impl ReposPage {
    /// The rail's "⋯" menu.
    pub fn toggle_repo_menu(&mut self, folder: &str, cx: &mut Context<Self>) {
        self.repo_menu = match self.repo_menu {
            Some(_) => None,
            None => Some(RepoMenu {
                folder: folder.to_string(),
                remove_armed: false,
            }),
        };
        cx.notify();
    }

    pub fn close_repo_menu(&mut self, cx: &mut Context<Self>) {
        if self.repo_menu.take().is_some() {
            cx.notify();
        }
    }

    /// The first click on the menu's Remove row; the second one removes.
    pub fn arm_repo_remove(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &mut self.repo_menu {
            menu.remove_armed = true;
            cx.notify();
        }
    }

    fn toggle_sort_menu(&mut self, cx: &mut Context<Self>) {
        self.sort_menu_open = !self.sort_menu_open;
        cx.notify();
    }

    /// An outside press closes whichever of the page's menus is open.
    pub fn close_popups(&mut self, cx: &mut Context<Self>) {
        self.close_repo_menu(cx);
        if self.sort_menu_open {
            self.sort_menu_open = false;
            cx.notify();
        }
    }

    /// Whether something sits on top of the list right now, so the window's
    /// row keymap stays out of it.
    pub fn layer_open(&self) -> bool {
        self.dialog.is_some() || self.repo_menu.is_some() || self.sort_menu_open
    }

    /// The "+ tag" chip asks for the inline editor; the input itself is
    /// created on the next frame, where the window is available.
    pub fn begin_tag_edit(&mut self, cx: &mut Context<Self>) {
        self.tag_editing = true;
        cx.notify();
    }

    pub fn cancel_tag_edit(&mut self, cx: &mut Context<Self>) {
        self.tag_editing = false;
        self.tag_draft = None;
        cx.notify();
    }

    /// Creates the tag input the frame the chip asked for it.
    fn ensure_tag_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tag_editing || self.tag_draft.is_some() {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("tag name"));
        input.update(cx, |state, cx| state.focus(window, cx));
        cx.subscribe(&input, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if !matches!(event, InputEvent::PressEnter { .. }) {
                return;
            }
            let value = this
                .tag_draft
                .as_ref()
                .map(|input| input.read(cx).value().trim().to_string())
                .unwrap_or_default();
            let folder = this.store.read(cx).focused_folder();
            if let (Some(folder), false) = (folder, value.is_empty()) {
                this.store
                    .update(cx, |s, cx| s.add_repo_tag(&folder, &value, cx));
            }
            this.tag_editing = false;
            this.tag_draft = None;
            cx.notify();
        })
        .detach();
        self.tag_draft = Some(input);
    }

    /// Escape closes the topmost page overlay. Returns whether one was
    /// actually open, so the window can fall through to its own layers.
    pub fn dismiss_overlay(&mut self, cx: &mut Context<Self>) -> bool {
        if self.dialog.is_some() {
            self.dialog = None;
            cx.notify();
            return true;
        }
        // the rail's inline editors close on Escape before anything else
        if self.tag_editing || self.tag_draft.is_some() {
            self.tag_editing = false;
            self.tag_draft = None;
            cx.notify();
            return true;
        }
        if self.repo_menu.take().is_some() {
            cx.notify();
            return true;
        }
        if self.sort_menu_open {
            self.sort_menu_open = false;
            cx.notify();
            return true;
        }
        false
    }

    /// Everything the column layout depends on; a change triggers one
    /// refresh and one rebuild of the shared [`TableLayout`].
    fn current_layout_key(&self, available: Pixels, cx: &Context<Self>) -> LayoutKey {
        LayoutKey::of(self.store.read(cx), available)
    }

    /// Sends the Add-Repositories dialog's deep scan.
    pub fn kick_deep_scan(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::AddRepo(dialog)) = &mut self.dialog else {
            return;
        };
        // Enter and the Rescan button both route here; a scan already in
        // flight must not be re-sent (two settles would flash two result sets)
        if dialog.scanning {
            return;
        }
        let root = dialog.root.read(cx).value().to_string();
        if root.trim().is_empty() {
            return;
        }
        let depth = dialog.depth;
        dialog.scanning = true;
        dialog.scanned = false;
        dialog.found.clear();
        let settings = self.store.read(cx).settings();
        let _ = self
            .store
            .read(cx)
            .net
            .send_blocking(crate::net::NetCommand::DeepScan {
                root: root.trim().to_string(),
                depth,
                settings: settings.repos.clone(),
            });
        cx.notify();
    }

    /// Adds every checked entry from the Add-Repositories dialog.
    pub fn commit_add_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::AddRepo(dialog)) = &mut self.dialog else {
            return;
        };
        let selected: Vec<devtools_core::scan::ScannedRepo> = dialog
            .found
            .iter()
            .filter(|(_, checked, tracked)| *checked && !*tracked)
            .map(|(repo, _, _)| repo.clone())
            .collect();
        if selected.is_empty() {
            self.dialog = None;
            cx.notify();
            return;
        }
        self.store
            .update(cx, |store, cx| store.register_repos(&selected, cx));
        self.dialog = None;
        cx.notify();
    }

    /// Sends the Clone dialog's clone (with inline validation).
    pub fn commit_clone(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Clone(dialog)) = &mut self.dialog else {
            return;
        };
        let mut validation_error: Option<String> = None;
        {
            let url = dialog.url.read(cx).value().to_string();
            let name = dialog.folder_name(cx);
            let destination = dialog.destination.read(cx).value().to_string();
            if url.trim().is_empty() || (!url.contains('/') && !url.contains(':')) {
                validation_error = Some("Enter a repository URL".into());
            } else if name.is_empty()
                || name == "."
                || name == ".."
                || name.contains('/')
                || name.contains('\\')
            {
                validation_error = Some("Enter a usable repository name".into());
            } else if destination.trim().is_empty() || !std::path::Path::new(&destination).is_dir()
            {
                validation_error = Some("Destination folder does not exist".into());
            } else if std::path::Path::new(&destination).join(&name).exists() {
                validation_error = Some("Target folder already exists".into());
            }
        }
        if let Some(error) = validation_error {
            dialog.busy = false;
            dialog.error = Some(error);
            cx.notify();
            return;
        }
        let url = dialog.url.read(cx).value().to_string();
        let name = dialog.folder_name(cx);
        let destination = dialog.destination.read(cx).value().to_string();
        dialog.busy = true;
        dialog.error = None;
        let _ = self
            .store
            .read(cx)
            .net
            .send_blocking(crate::net::NetCommand::Clone {
                url: url.trim().to_string(),
                parent_dir: destination,
                name,
            });
        cx.notify();
    }
}

/// The old data table stays reachable for comparison while the redesign
/// lands: `DEVTOOLS_UI_TABLE=1`. Everything else about this page is the list
/// plus the detail rail, and the workspace bar only rides with the table.
///
/// Read per call rather than cached: the probes flip it at runtime, and an
/// environment lookup next to a full frame layout does not show up.
pub fn table_requested() -> bool {
    std::env::var("DEVTOOLS_UI_TABLE").is_ok_and(|value| value == "1")
}

/// Keeps the split screen's selection alive; the old table stays out of it
/// because its workspace bar closes by clearing the selection.
fn keep_row_selected(store: &Entity<Store>, cx: &mut App) {
    if !table_requested() {
        store.update(cx, |s, cx| s.ensure_row_selected(cx));
    }
}

impl Render for ReposPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // drain dialog-bound results the moment we are about to draw
        let drained_deep_scan = self
            .store
            .update(cx, |store, _| store.deep_scan_result.take());
        if let Some(found) = drained_deep_scan
            && let Some(Dialog::AddRepo(dialog)) = &mut self.dialog
        {
            dialog.scanning = false;
            dialog.scanned = true;
            dialog.found = found
                .into_iter()
                .map(|repo| {
                    let tracked = self
                        .store
                        .read(cx)
                        .repos
                        .iter()
                        .any(|r| r.folder_path == repo.folder_path);
                    (repo, !tracked, tracked)
                })
                .collect();
        }
        if let Some(path) = self
            .store
            .update(cx, |store, _| store.folder_pick_result.take())
            && let Some(Dialog::AddRepo(dialog)) = &mut self.dialog
        {
            dialog
                .root
                .update(cx, |field, cx| field.set_value(&path, window, cx));
        }
        let drained_clone = self.store.update(cx, |store, _| store.clone_result.take());
        if let Some((parent_dir, name, result)) = drained_clone {
            if result.success {
                self.store
                    .update(cx, |store, cx| store.adopt_clone(&parent_dir, &name, cx));
                self.dialog = None;
            } else if let Some(Dialog::Clone(dialog)) = &mut self.dialog {
                dialog.busy = false;
                dialog.error = result.error.or_else(|| Some("clone failed".into()));
            }
        }
        // The split layout is the app: the list on the left, the detail rail
        // on the right. The old data table stays reachable with
        // `DEVTOOLS_UI_TABLE=1` while the last behaviours move over.
        //
        // A dialog is a layer on top of whichever layout is underneath, never
        // a reason to swap it: this branch used to be skipped while a dialog
        // was open, so Add repo / Clone dropped the user back onto the legacy
        // table until the dialog closed.
        if !table_requested() {
            self.ensure_tag_editor(window, cx);
            let list_w = window.viewport_size().width - px(crate::ui::inspector::RAIL_W);
            let toolbar = self.render_toolbar(list_w, cx);
            let sort_menu = self.render_sort_menu(cx);
            let mut page = {
                let store = self.store.read(cx);
                let list = crate::ui::repos_proto::render(store, &self.store);
                let footer = key_hints(store);
                let page = cx.entity();
                let tag_draft = self.tag_draft.clone();
                let repo_menu = self.repo_menu.as_ref();
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .relative()
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .min_h_0()
                                    .child(toolbar)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_h_0()
                                            .relative()
                                            .child(list)
                                            // last, so it paints over the rows
                                            .children(sort_menu),
                                    ),
                            )
                            .child(crate::ui::inspector::render(
                                store,
                                &self.store,
                                &page,
                                tag_draft.as_ref(),
                                repo_menu,
                            )),
                    )
                    .child(footer)
            };
            if let Some(dialog) = &mut self.dialog {
                page = page.child(dialog.render(window, cx));
            }
            return page.into_any_element();
        }
        // Dynamic columns: the kit's table only re-prepares column widths on
        // refresh(), so feed it the real available width and refresh once per
        // key change (window resize, settings, widest branch, sort). The same
        // pass rebuilds the delegate's shared layout — its per-cell work then
        // stays O(1) for the rest of the frame's lifetime.
        let available = window.viewport_size().width
            - px(32.) // the px_4 gutters around the table
            - px(14.); // vertical scrollbar + slack
        let key = self.current_layout_key(available, cx);
        if key != self.layout_fingerprint.get() {
            self.layout_fingerprint.set(key);
            self.available_width.set(available);
            let layout = {
                let store = self.store.read(cx);
                let settings = Rc::new(store.settings());
                let branch = branch_column_width(&self.store, cx);
                let cols = visible_columns(&settings, available, branch);
                let metrics = layout_metrics(&settings, available, branch);
                let name = name_width(&cols, &settings, available, metrics);
                Rc::new(TableLayout {
                    key,
                    cols,
                    metrics,
                    name,
                    settings,
                })
            };
            *self.table_layout.borrow_mut() = Some(layout);
            self.table.update(cx, |state, cx| state.refresh(cx));
        }
        let mut page = div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .relative()
            .child(self.render_toolbar(window.viewport_size().width, cx))
            .child(
                div()
                    .debug_selector(|| "repos-table-wrap".into())
                    .flex_1()
                    .min_h_0()
                    .px_4()
                    .pb_2()
                    .child(
                        DataTable::new(&self.table)
                            .with_size(Size::Size(px(theme::TABLE_ROW_H)))
                            .stripe(false)
                            .bordered(false)
                            // the columns always sum to the available width
                            // (the name column absorbs the rest), so the kit's
                            // always-painted horizontal bar is pure noise
                            .scrollbar_visible(true, false),
                    ),
            );
        if let Some(dialog) = &mut self.dialog {
            page = page.child(dialog.render(window, cx));
        }
        page.into_any_element()
    }
}

impl ReposPage {
    fn render_toolbar(
        &mut self,
        viewport_width: Pixels,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (tags, active_tags, scanning) = {
            let store = self.store.read(cx);
            (store.all_tags(), store.tag_filters.clone(), store.scanning)
        };

        let (archived_count, show_archived) = {
            let store = self.store.read(cx);
            (
                store.repos.iter().filter(|r| r.is_archived()).count(),
                store.show_archived,
            )
        };

        let stats = {
            let store = self.store.read(cx);
            // totals describe the live list: archived repos leave every sum
            let mut stats = ListStats::default();
            for repo in store.repos.iter().filter(|r| !r.is_archived()) {
                stats.total += 1;
                stats.changes += repo.git_modified_count;
                stats.issues += repo.github_issue_count;
                if repo.is_favorite() {
                    stats.favorites += 1;
                }
                if crate::store::projection::needs_attention(repo) {
                    stats.attention += 1;
                }
            }
            stats
        };
        let attention_only = self.store.read(cx).attention_only;
        let filters_active =
            !active_tags.is_empty() || self.store.read(cx).group_filter.is_some() || attention_only;

        // The pane head sheds its subtitle first when the list column gets
        // narrow (the same numbers ride the rows), then the tag chips; the
        // action buttons never leave.
        let scale = theme::font_small() / 14.0;
        // both rows still fit a 620px column: title + three ghost actions on
        // the head, the scope control + chips below
        let compact = viewport_width < px(620.) * scale;
        let show_subtitle = !compact;
        let show_tag_chips = viewport_width >= px(620.) * scale;

        // Tag filters read as chips: raised when active, quiet otherwise (OR
        // semantics across the checked set); each carries its live count.
        // The export's chip row carries user tags that actually match
        // something: the derived flags (favourites/archived) have their own
        // scope segments and a zero-count chip is noise.
        let tag_counts: Vec<(String, usize)> = {
            let store = self.store.read(cx);
            tags.iter()
                .filter(|tag| {
                    !tag.eq_ignore_ascii_case(devtools_core::repo_store::FAVORITES_TAG)
                        && !tag.eq_ignore_ascii_case(devtools_core::repo_store::ARCHIVED_TAG)
                })
                .map(|tag| {
                    let count = store
                        .repos
                        .iter()
                        .filter(|repo| !repo.is_archived())
                        .filter(|repo| repo.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
                        .count();
                    (tag.clone(), count)
                })
                .filter(|(_, count)| *count > 0)
                .collect()
        };
        let mut tag_pills = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .min_w_0()
            .overflow_hidden();
        for (index, (tag, count)) in tag_counts.iter().enumerate() {
            let active = active_tags.iter().any(|t| t.eq_ignore_ascii_case(tag));
            tag_pills = tag_pills.child(
                div()
                    .id(gpui::ElementId::NamedInteger("filter".into(), index as u64))
                    .a11y_toggle(tag.clone(), active)
                    .cursor_pointer()
                    .rounded_sm()
                    .border_1()
                    .border_color(if active {
                        theme::LINE_STRONG
                    } else {
                        theme::HAIRLINE
                    })
                    .bg(if active {
                        theme::CHIP
                    } else {
                        theme::BACKGROUND
                    })
                    .px_2()
                    .py_0p5()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::font_meta()))
                    .when(active, |d| d.text_color(theme::TEXT))
                    .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                    .hover(|s| s.text_color(theme::TEXT).border_color(theme::LINE_STRONG))
                    .on_click({
                        let tag = tag.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.store.update(cx, |s, cx| s.toggle_tag_filter(&tag, cx));
                        })
                    })
                    .child(tag.clone())
                    .child(
                        div()
                            .text_size(px(theme::font_meta()))
                            .text_color(theme::TEXT_FAINT)
                            .child(format!("{count}")),
                    ),
            );
        }

        let store_refresh = self.store.clone();
        let actions = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .flex_shrink_0()
            .child(toolbar_action(
                "refresh-button",
                if scanning { "Refreshing" } else { "Refresh" },
                Some("icons/icon-refresh.svg"),
                false,
                move |_, _, cx| {
                    store_refresh.update(cx, |s, cx| s.refresh(cx));
                },
            ))
            .child(toolbar_action(
                "add-repo-button",
                "Add repo",
                Some("icons/icon-plus.svg"),
                false,
                cx.listener(|this, _, window, cx| {
                    let depth = this.store.read(cx).settings().repos.max_scan_depth;
                    this.dialog = Some(Dialog::AddRepo(AddRepoDialog::new(window, depth, cx)));
                    cx.notify();
                }),
            ))
            .child(toolbar_action(
                "clone-button",
                "Clone",
                Some("icons/icon-cloud-download.svg"),
                false,
                cx.listener(|this, _, window, cx| {
                    let store = this.store.clone();
                    this.dialog = Some(Dialog::Clone(CloneDialog::new(&store, window, cx)));
                    cx.notify();
                }),
            ));

        // The subtitle summarises the whole list: what needs a hand first,
        // then the two totals that make a repo look busy.
        let mut subtitle = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(7.))
            .text_size(px(theme::font_sub()));
        if stats.attention > 0 {
            subtitle = subtitle.child(div().text_color(theme::AMBER_BRIGHT).child(format!(
                "{} of {} need attention",
                stats.attention, stats.total
            )));
        } else {
            subtitle = subtitle.child(
                div()
                    .text_color(theme::TEXT_MUTED)
                    .child(format!("{} repos", stats.total)),
            );
        }
        subtitle = subtitle
            .child(dot())
            .child(
                div()
                    .text_color(theme::TEXT_MUTED)
                    .child(format!("{} changes", stats.changes)),
            )
            .child(dot())
            .child(
                div()
                    .text_color(theme::TEXT_MUTED)
                    .child(format!("{} issues", stats.issues)),
            );

        let header_row = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .px(px(20.))
            .pt(px(16.))
            .pb(px(10.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(theme::font_screen_title()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child("Repositories"),
                    )
                    .when(show_subtitle, |head| head.child(subtitle)),
            )
            .child(actions);

        // Scope segmented control: one row of filters, counts included.
        let scope = segment_row()
            .child(segment(
                "scope-all",
                "All",
                Some(stats.total),
                !attention_only && active_tags.is_empty(),
                cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| {
                        s.clear_filters(cx);
                        if s.show_archived {
                            s.toggle_show_archived(cx);
                        }
                    });
                }),
            ))
            .child(segment(
                "scope-attention",
                "Needs attention",
                Some(stats.attention),
                attention_only,
                cx.listener(|this, _, _, cx| {
                    this.store
                        .update(cx, |s, cx| s.set_attention_only(true, cx));
                }),
            ))
            .child(segment(
                "scope-favorites",
                "Favorites",
                Some(stats.favorites),
                active_tags
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(devtools_core::repo_store::FAVORITES_TAG)),
                cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| {
                        s.toggle_tag_filter(devtools_core::repo_store::FAVORITES_TAG, cx);
                    });
                }),
            ))
            .child(segment(
                "archive-chip",
                "Archive",
                Some(archived_count),
                show_archived,
                cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| s.toggle_show_archived(cx));
                }),
            ));

        // the table sorts from its headers
        let sort_button = (!table_requested()).then(|| {
            let (mode, ascending) = {
                let store = self.store.read(cx);
                (store.sort_mode, store.sort_ascending)
            };
            div()
                .flex_shrink_0()
                // keep the window's outside-click dismissal from seeing this
                // press: it must toggle, not close-then-reopen
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(toolbar_action(
                    "sort-button",
                    format!("{} {}", sort_label(mode), sort_arrow(ascending)),
                    None,
                    false,
                    cx.listener(|this, _, _, cx| this.toggle_sort_menu(cx)),
                ))
        });

        let store_clear = self.store.clone();
        // the chips wrap under the scope control when the column is narrow
        // instead of clipping at its edge
        let scope_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .h(px(36.))
            .px(px(20.))
            .border_b_1()
            .border_color(theme::HAIRLINE)
            .child(scope)
            .child(div().flex_1())
            .when(show_tag_chips, |row| row.child(tag_pills))
            .when(filters_active, |row| {
                row.child(
                    div()
                        .id("clear-button")
                        .debug_selector(|| "clear-button".into())
                        .a11y_button("Clear filters")
                        .cursor_pointer()
                        .rounded_sm()
                        .px_2()
                        .py_0p5()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::TEXT_MUTED)
                        .hover(|s| s.text_color(theme::TEXT))
                        .on_click(move |_, _, cx| {
                            store_clear.update(cx, |s, cx| s.clear_filters(cx));
                        })
                        .child("Clear"),
                )
            })
            .children(sort_button);

        div()
            .flex()
            .flex_col()
            .relative()
            .child(header_row)
            .child(scope_row)
            .into_any_element()
    }

    /// The list head's sort menu, hanging under its button at the list's top
    /// right. Picking the active mode flips its direction.
    fn render_sort_menu(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.sort_menu_open {
            return None;
        }
        let (current, ascending, github) = {
            let store = self.store.read(cx);
            (
                store.sort_mode,
                store.sort_ascending,
                store.settings().repos.enable_git_hub,
            )
        };
        let mut menu = div()
            .id("sort-menu")
            .debug_selector(|| "sort-menu".into())
            .absolute()
            .top(px(4.))
            .right(px(20.))
            .w(px(170.))
            .p(px(4.))
            .rounded(px(6.))
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .shadow_md()
            // presses inside must not reach the rows under it, or the
            // window's outside-click dismissal before the row's click lands
            .occlude()
            .flex()
            .flex_col()
            .gap(px(2.));
        for (mode, label) in SORT_MODES {
            if mode == RepoSortMode::GitHub && !github {
                continue;
            }
            let active = mode == current;
            let selector = format!("sort-mode-{label}");
            menu = menu.child(
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .a11y_toggle(label, active)
                    .cursor_pointer()
                    .rounded(px(4.))
                    .px(px(8.))
                    .py(px(5.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(px(theme::font_row_meta()))
                    .text_color(if active { theme::TEXT } else { theme::TEXT_DIM })
                    .hover(|d| d.bg(theme::HOVER_BG).text_color(theme::TEXT))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store.update(cx, |s, cx| {
                            let ascending = if s.sort_mode == mode {
                                !s.sort_ascending
                            } else {
                                mode.default_ascending()
                            };
                            s.set_sort_direction(mode, ascending, cx);
                        });
                        this.sort_menu_open = false;
                        cx.notify();
                    }))
                    .child(label)
                    .when(active, |row| row.child(sort_arrow(ascending))),
            );
        }
        Some(menu.into_any_element())
    }
}

/// The sort menu's modes, in its order.
const SORT_MODES: [(RepoSortMode, &str); 6] = [
    (RepoSortMode::Attention, "Attention first"),
    (RepoSortMode::LastActivity, "Last activity"),
    (RepoSortMode::Changes, "Changes"),
    (RepoSortMode::GitHub, "GitHub"),
    (RepoSortMode::Name, "Name"),
    (RepoSortMode::Branch, "Branch"),
];

fn sort_label(mode: RepoSortMode) -> &'static str {
    SORT_MODES
        .iter()
        .find(|(candidate, _)| *candidate == mode)
        .map_or("Name", |(_, label)| label)
}

fn sort_arrow(ascending: bool) -> &'static str {
    if ascending { "↑" } else { "↓" }
}

/// The 2px bullet the export separates the summary parts with.
fn dot() -> gpui::AnyElement {
    div()
        .size(px(2.))
        .rounded_full()
        .bg(theme::TEXT_FAINT)
        .into_any_element()
}

/// The footer strip: the keyboard grammar on the left, the same totals the
/// pane head carries on the right.
fn key_hints(store: &Store) -> gpui::AnyElement {
    let mut attention = 0usize;
    let mut changes = 0u32;
    let mut issues = 0u32;
    for repo in store.repos.iter().filter(|repo| !repo.is_archived()) {
        changes += repo.git_modified_count;
        issues += repo.github_issue_count;
        if crate::store::projection::needs_attention(repo) {
            attention += 1;
        }
    }

    let mut hints = div().flex().items_center().gap_3();
    for (keys, label) in [
        ("\u{2191}\u{2193}", "Rows"),
        ("\u{21b5}", "Open"),
        ("S", "Favorite"),
        ("C", "Commit"),
        ("Ctrl K", "Search"),
    ] {
        hints = hints.child(
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(keycap(keys))
                .child(div().text_color(theme::TEXT_MUTED).child(label)),
        );
    }

    div()
        .debug_selector(|| "repos-footer".into())
        .flex()
        .items_center()
        .justify_between()
        .h(px(26.))
        .px(px(12.))
        .border_t_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::FOOT_BG)
        .text_size(px(theme::font_row_meta()))
        .text_color(theme::TEXT_MUTED)
        .child(hints)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .when(attention > 0, |right| {
                    right.child(
                        div()
                            .text_color(theme::AMBER_BRIGHT)
                            .child(format!("{attention} need attention")),
                    )
                })
                .child(div().text_color(theme::TEXT_FAINT).child("\u{00b7}"))
                .child(
                    div()
                        .text_color(theme::TEXT_MUTED)
                        .child(format!("{changes} changes")),
                )
                .child(div().text_color(theme::TEXT_FAINT).child("\u{00b7}"))
                .child(
                    div()
                        .text_color(theme::TEXT_MUTED)
                        .child(format!("{issues} issues")),
                ),
        )
        .into_any_element()
}

fn keycap(label: &'static str) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(15.))
        .min_w(px(16.))
        .px(px(4.))
        .rounded(px(3.))
        .border_1()
        .border_color(theme::KEY_BORDER)
        .bg(theme::KEY_BG)
        .font_family(theme::code_font())
        .text_size(px(theme::font_micro()))
        .text_color(theme::KEY_FG)
        .child(label)
        .into_any_element()
}

/// A labelled pane-head button: filled when primary, ghost otherwise. The
/// ghost is the default here — the design has one filled action at a time and
/// that one lives in the detail rail.
fn toolbar_action(
    id: &'static str,
    label: impl Into<gpui::SharedString>,
    icon_path: Option<&'static str>,
    primary: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    let label: gpui::SharedString = label.into();
    let color = if primary {
        theme::ON_ACCENT
    } else {
        theme::TEXT_DIM
    };
    div()
        .id(id)
        .a11y_button(label.clone())
        .cursor_pointer()
        .rounded(px(4.))
        .h(px(26.))
        .px(px(8.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(5.))
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .debug_selector(move || id.into())
        .when(primary, |d| d.bg(theme::ACCENT).hover(|s| s.opacity(0.9)))
        .when(!primary, |d| {
            d.bg(theme::GHOST_BG)
                .border_1()
                .border_color(theme::GHOST_BORDER)
                .hover(|s| {
                    s.bg(theme::HOVER_BG)
                        .border_color(theme::GHOST_BORDER_HOVER)
                        .text_color(theme::TEXT)
                })
        })
        .on_click(on_click)
        .children(icon_path.map(|path| icon(path, px(theme::icon_small() - 1.), color)))
        .child(label)
        .into_any_element()
}

/// The segmented control's trough: the list's scope row and the workspace
/// sheet's tabs.
pub(crate) fn segment_row() -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_0p5()
        .h(px(26.))
        .p(px(2.))
        .rounded(px(5.))
        .border_1()
        .border_color(theme::SEG_BORDER)
        .bg(theme::SEG_BG)
}

/// One segment: label, count when it has one, and the raised fill when it is
/// the active choice.
pub(crate) fn segment(
    id: &'static str,
    label: &'static str,
    count: Option<usize>,
    active: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .a11y_toggle(
            match count {
                Some(count) => format!("{label} ({count})"),
                None => label.to_string(),
            },
            active,
        )
        .debug_selector(move || id.into())
        .cursor_pointer()
        .rounded(px(3.))
        .h(px(20.))
        .px(px(7.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(4.))
        .text_size(px(theme::font_row_meta()))
        .when(active, |d| {
            d.bg(theme::SEG_ACTIVE).text_color(theme::SEG_ACTIVE_FG)
        })
        .when(!active, |d| {
            d.text_color(theme::TEXT_MUTED)
                .hover(|s| s.text_color(theme::TEXT))
        })
        .on_click(on_click)
        .child(label)
        .children(count.map(|count| {
            div()
                .text_size(px(theme::font_meta()))
                .text_color(theme::TEXT_FAINT)
                .child(format!("{count}"))
        }))
        .into_any_element()
}

/// The shortcuts column hugs its button row: 36px buttons, 6px gaps, and the
/// CLI group's divider (1px + mx_1). Follows the same settings gates as
/// `shortcuts_cell`.
fn shortcuts_column_width(settings: &AppSettings) -> Pixels {
    let mut buttons = 1usize; // open folder is always present
    if settings.repos.enable_terminal {
        buttons += 1;
    }
    if settings.repos.enable_vscode {
        buttons += 1;
    }
    if cfg!(windows) && settings.repos.enable_visual_studio {
        buttons += 1;
    }
    if settings.warp_agent.enable_warp_agent {
        buttons += 1; // the warp-agent door
    }
    let mut cli = 0usize;
    if settings.opencode.enable_opencode {
        cli += 1;
    }
    if settings.deepseek.enable_deepseek {
        cli += 1;
    }
    let divider = cli > 0;
    buttons += cli;
    let children = buttons + divider as usize;
    let gaps = children.saturating_sub(1) as f32 * 6.0;
    px(buttons as f32 * 36.0 + if divider { 9.0 } else { 0.0 } + gaps + 8.0)
}

/// Branch column width from the widest branch pill: git glyph, pill padding,
/// ~7.8px per character in the code font. Labels longer than
/// BRANCH_LABEL_CHARS render trimmed, so the widest label sets the width.
fn branch_column_width(store: &Entity<Store>, cx: &App) -> Pixels {
    let chars =
        (store.read(cx).widest_branch_chars as f32).min(crate::ui::BRANCH_LABEL_CHARS as f32);
    px(56. + chars * 7.8).max(px(110.))
}

// ---- the table --------------------------------------------------------------

/// Which column sits at an index — the GitHub/Azure columns exist only when
/// their settings enable them, so everything resolves through this. Row
/// actions (the launcher buttons) sit right after the name column, in the
/// dead space the folder path leaves, so the eye scans identity → actions →
/// state → time.
#[derive(Clone, Copy, PartialEq)]
enum ColKind {
    Star,
    Name,
    Branch,
    Changes,
    Github,
    Azure,
    Activity,
    Shortcuts,
}

/// The columns that fit the current width. Identity, actions and state (star,
/// name, launcher buttons, branch, changes) always stay; the activity
/// timestamp, then the Azure status, then the GitHub counters drop out as the
/// window narrows. Without this the fixed columns squeeze the name column to
/// its floor and spill a horizontal scrollbar onto the table.
fn visible_columns(settings: &AppSettings, available: Pixels, branch: Pixels) -> Vec<ColKind> {
    let scale = theme::font_small() / 12.0;
    let star = px(36.);
    let shortcuts = shortcuts_column_width(settings);
    let changes = px(132.) * scale;
    let activity = px(84.) * scale;
    let github = if settings.repos.enable_git_hub {
        px(96.) * scale
    } else {
        px(0.)
    };
    let azure = if settings.repos.enable_azure_dev_ops {
        px(120.) * scale
    } else {
        px(0.)
    };
    // what the name column needs to stay readable next to the fixed ones
    let required = |extra: Pixels| star + shortcuts + changes + branch + px(140.) + extra;

    let mut show_activity = true;
    let mut show_azure = settings.repos.enable_azure_dev_ops;
    let mut show_github = settings.repos.enable_git_hub;

    if available < required(activity + github + azure) {
        show_activity = false;
    }
    if available < required(github + azure) {
        show_azure = false;
    }
    if available < required(github) {
        show_github = false;
    }

    let mut cols = vec![
        ColKind::Star,
        ColKind::Name,
        ColKind::Shortcuts,
        ColKind::Branch,
        ColKind::Changes,
    ];
    if show_github {
        cols.push(ColKind::Github);
    }
    if show_azure {
        cols.push(ColKind::Azure);
    }
    if show_activity {
        cols.push(ColKind::Activity);
    }
    cols
}

/// The widths the table's fixed columns get. `changes` is the rendered
/// changes-column width: at narrow sizes the word "changes"/"clean" is
/// dropped (the header still labels the column) and the room goes to the
/// repository name.
#[derive(Clone, Copy)]
struct LayoutMetrics {
    branch: Pixels,
    changes: Pixels,
    compact_changes: bool,
}

/// The width of one column kind. The name column is not covered here — it
/// absorbs whatever the others leave over.
fn column_width(kind: ColKind, settings: &AppSettings, m: LayoutMetrics) -> Pixels {
    let scale = theme::font_small() / 12.0;
    match kind {
        ColKind::Star => px(36.),
        ColKind::Name => px(0.),
        ColKind::Branch => m.branch,
        ColKind::Changes => m.changes,
        ColKind::Github => px(96.) * scale,
        ColKind::Azure => px(120.) * scale,
        ColKind::Activity => px(84.) * scale,
        ColKind::Shortcuts => shortcuts_column_width(settings),
    }
}

/// Picks the fixed widths for the current width. The column *visibility* is
/// decided with the full changes column (so narrowing never brings a hidden
/// column back); the compact changes cell only kicks in once the name column
/// would otherwise be squeezed below a readable 200px.
fn layout_metrics(settings: &AppSettings, available: Pixels, branch: Pixels) -> LayoutMetrics {
    let scale = theme::font_small() / 12.0;
    let full = LayoutMetrics {
        branch,
        changes: px(132.) * scale,
        compact_changes: false,
    };
    let cols = visible_columns(settings, available, branch);
    let name = name_width(&cols, settings, available, full);
    if name < px(200.) {
        LayoutMetrics {
            branch,
            changes: px(86.) * scale,
            compact_changes: true,
        }
    } else {
        full
    }
}

/// The name column's share of the table width: whatever the other visible
/// columns leave over, floored so it never disappears.
fn name_width(
    cols: &[ColKind],
    settings: &AppSettings,
    available: Pixels,
    m: LayoutMetrics,
) -> Pixels {
    let fixed = cols
        .iter()
        .filter(|kind| **kind != ColKind::Name)
        .fold(px(0.), |sum, kind| sum + column_width(*kind, settings, m));
    (available - fixed).max(px(140.))
}

pub struct ReposDelegate {
    pub store: Entity<Store>,
    /// The table's available width, kept current by the page's render; see
    /// `ReposPage::available_width`.
    pub available_width: Rc<Cell<Pixels>>,
    /// The page's cached column layout (see [`TableLayout`]); every per-cell
    /// read goes through it instead of rescanning the store.
    layout: Rc<RefCell<Option<Rc<TableLayout>>>>,
}

impl ReposDelegate {
    pub fn new(store: Entity<Store>, available: Pixels) -> Self {
        Self {
            store,
            available_width: Rc::new(Cell::new(available)),
            layout: Rc::new(RefCell::new(None)),
        }
    }

    fn repo_at(&self, row_ix: usize, cx: &App) -> Option<RepoEntry> {
        let store = self.store.read(cx);
        store
            .repo_row(row_ix)
            .and_then(|ix| store.repos.get(ix))
            .cloned()
    }

    fn row_is_group(&self, row_ix: usize, cx: &App) -> bool {
        self.store.read(cx).row_is_group(row_ix)
    }

    fn folder_at(&self, row_ix: usize, cx: &App) -> Option<String> {
        self.repo_at(row_ix, cx)
            .map(|r| r.folder_path.to_string_lossy().to_string())
    }

    /// Inverse of [`Self::folder_at`]: the visible row for a folder, used to
    /// mirror the store's selection into the table highlight. A folder whose
    /// group is collapsed projects no row — the caller clears the highlight.
    fn row_for_folder(&self, folder: &str, cx: &App) -> Option<usize> {
        let store = self.store.read(cx);
        let folder = std::path::Path::new(folder);
        store.projected.iter().position(|row| {
            matches!(
                row,
                ProjectedRow::Repo(ix)
                    if store.repos.get(*ix).is_some_and(|repo| repo.folder_path == folder)
            )
        })
    }

    /// The shared layout; rebuilt inline when the cached one is stale or
    /// absent (first frame, probes, or a key the page has not refreshed yet).
    fn layout(&self, cx: &App) -> Rc<TableLayout> {
        let key = LayoutKey::of(self.store.read(cx), self.available_width.get());
        if let Some(layout) = self.layout.borrow().as_ref()
            && layout.key == key
        {
            return layout.clone();
        }
        let settings = Rc::new(self.store.read(cx).settings());
        let branch = branch_column_width(&self.store, cx);
        let available = self.available_width.get();
        let cols = visible_columns(&settings, available, branch);
        let metrics = layout_metrics(&settings, available, branch);
        let name = name_width(&cols, &settings, available, metrics);
        let layout = Rc::new(TableLayout {
            key,
            cols,
            metrics,
            name,
            settings,
        });
        *self.layout.borrow_mut() = Some(layout.clone());
        layout
    }

    /// Which column sits at an index given the current width and settings.
    fn kind(&self, col_ix: usize, cx: &App) -> ColKind {
        self.layout(cx)
            .cols
            .get(col_ix)
            .copied()
            .unwrap_or(ColKind::Shortcuts)
    }
}

impl TableDelegate for ReposDelegate {
    /// A compact column-header strip: the labels explain the icon cluster,
    /// the branch pill and the quiet "—" cells, and the sortable headers are
    /// the page's only sort controls (mode + direction cycle per column).
    fn render_header(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("repos-header")
            .debug_selector(|| "repos-header".into())
            .h(px(theme::TABLE_HEAD_H))
            .border_b_1()
            .border_color(theme::BORDER_LIGHT)
            .text_color(theme::TEXT_MUTED)
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let label = match self.kind(col_ix, cx) {
            ColKind::Name => "Repository",
            ColKind::Branch => "Branch",
            ColKind::Changes => "Changes",
            ColKind::Github => "GitHub",
            ColKind::Azure => "Azure DevOps",
            ColKind::Activity => "Last activity",
            ColKind::Star | ColKind::Shortcuts => "",
        };
        div()
            .size_full()
            .flex()
            .items_center()
            .text_size(px(theme::font_small() - 2.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme::TEXT_MUTED)
            .child(label)
    }

    fn columns_count(&self, cx: &App) -> usize {
        self.layout(cx).cols.len()
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.store.read(cx).projected.len()
    }

    /// Group header rows paint their own band: the kit's hover/selection
    /// backgrounds sit under the cells, so an opaque full-row overlay pinned
    /// above them keeps the header visually flat at any pointer position.
    /// Repo rows keep the kit's default row chrome.
    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> gpui::Stateful<gpui::Div> {
        let group = {
            let store = self.store.read(cx);
            match store.projected.get(row_ix) {
                Some(ProjectedRow::Group {
                    key,
                    name,
                    count,
                    collapsed,
                }) => Some((key.clone(), name.clone(), *count, *collapsed)),
                _ => None,
            }
        };
        let repo_label = {
            let store = self.store.read(cx);
            store
                .projected
                .get(row_ix)
                .and_then(|row| row.repo_index())
                .and_then(|ix| store.repos.get(ix))
                .map(|repo| (repo.name.clone(), repo.is_archived()))
        };
        let mut tr = div().id(("row", row_ix));
        if let Some((key, name, count, collapsed)) = group {
            let selector = format!("repos-group-header-{key}");
            tr = tr
                .aria_label(format!("{name} — {count} repositories"))
                .aria_expanded(!collapsed)
                .relative()
                // the hover-revealed solo icon inside the cells keys off this group
                .group("repos-group-header")
                .debug_selector(move || selector.clone())
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(theme::CARD)
                        .border_t_1()
                        .border_color(theme::BORDER_LIGHT)
                        .border_b_1()
                        .border_color(theme::BORDER_LIGHT),
                );
        } else {
            if let Some((name, archived)) = repo_label {
                tr = tr.aria_label(if archived {
                    format!("{name} (archived)")
                } else {
                    name
                });
            }
            // repo rows register the hover group the activity cell's
            // archive/restore action reveals under — on the row, so hovering
            // anywhere on it counts, not just that one cell
            tr = tr.group("repo-row");
        }
        tr
    }

    fn column(&self, col_ix: usize, cx: &App) -> Column {
        // fixed-width columns scale with the font factor so scaled text keeps
        // fitting; the name column absorbs whatever is left over
        let layout = self.layout(cx);
        let scale = theme::font_small() / 12.0;
        // the header's sort glyph and its click cycle (default → descending →
        // ascending) run off this state, so it must mirror the store, not
        // start blank every rebuild
        let kit_sort = |mode: RepoSortMode| -> ColumnSort {
            let store = self.store.read(cx);
            if store.sort_mode == mode {
                if store.sort_ascending {
                    ColumnSort::Ascending
                } else {
                    ColumnSort::Descending
                }
            } else {
                ColumnSort::Default
            }
        };
        match layout
            .cols
            .get(col_ix)
            .copied()
            .unwrap_or(ColKind::Shortcuts)
        {
            ColKind::Star => Column::new("star", "")
                .width(px(36.))
                .resizable(false)
                .text_center(),
            ColKind::Name => Column::new("name", "Repository")
                .width(layout.name)
                .min_width(px(140.))
                .sort(kit_sort(RepoSortMode::Name)),
            ColKind::Branch => Column::new("branch", "Branch")
                .width(layout.metrics.branch)
                .resizable(false)
                .sort(kit_sort(RepoSortMode::Branch)),
            ColKind::Changes => Column::new("changes", "Changes")
                .width(layout.metrics.changes)
                .resizable(false)
                .sort(kit_sort(RepoSortMode::Changes)),
            ColKind::Github => Column::new("github", "GitHub")
                .width(px(96.) * scale)
                .sort(kit_sort(RepoSortMode::GitHub)),
            ColKind::Azure => Column::new("azure", "Azure DevOps")
                .width(px(120.) * scale)
                .text_center()
                .resizable(false),
            ColKind::Activity => Column::new("activity", "Last activity")
                .width(px(84.) * scale)
                .resizable(false)
                .sort(kit_sort(RepoSortMode::LastActivity)),
            ColKind::Shortcuts => {
                // hugs its button row: the width follows the enabled launchers
                Column::new("shortcuts", "")
                    .width(shortcuts_column_width(&layout.settings))
                    .resizable(false)
            }
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let mode = match self.kind(col_ix, cx) {
            ColKind::Name => Some(RepoSortMode::Name),
            ColKind::Branch => Some(RepoSortMode::Branch),
            ColKind::Changes => Some(RepoSortMode::Changes),
            ColKind::Github => Some(RepoSortMode::GitHub),
            ColKind::Activity => Some(RepoSortMode::LastActivity),
            _ => None,
        };
        let Some(mode) = mode else { return };
        // the kit's third click lands on Default ("unsorted" chevron); the
        // table always sorts, so that settles back to the mode's default
        let ascending = match sort {
            ColumnSort::Ascending => true,
            ColumnSort::Descending => false,
            ColumnSort::Default => mode.default_ascending(),
        };
        let store = self.store.clone();
        store.update(cx, |s, cx| s.set_sort_direction(mode, ascending, cx));
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let layout = self.layout(cx);
        let settings = layout.settings.clone();
        let store = self.store.read(cx);
        let Some(row) = store.projected.get(row_ix) else {
            return div().into_any_element();
        };
        // a group header row renders the same band in every column: the
        // chevron cell, the identity cell, then empties
        if let ProjectedRow::Group { .. } = row {
            let group = row.clone();
            let store = self.store.clone();
            let show_tooltips = settings.general.show_tooltips;
            return group_header_cell(&group, row_ix, col_ix, &store, show_tooltips);
        }
        let Some(repo) = row.repo_index().and_then(|ix| store.repos.get(ix)) else {
            return div().into_any_element();
        };
        let folder = repo.folder_path.to_string_lossy().to_string();
        let show_tooltips = settings.general.show_tooltips;
        let notes_enabled = settings.warp_notes.enable_warp_notes;
        let archived = repo.is_archived();
        let warp_running = store
            .warp_running
            .get(&folder)
            .filter(|status| !status.running.is_empty())
            .map(|status| status.running.join(", "));

        let cell: gpui::AnyElement = match layout
            .cols
            .get(col_ix)
            .copied()
            .unwrap_or(ColKind::Shortcuts)
        {
            ColKind::Star => {
                if archived {
                    // archived rows are inert: no star, no toggle
                    div().into_any_element()
                } else {
                    let favorite = repo.is_favorite();
                    let star = div()
                        .id(("star", row_ix))
                        .debug_selector(move || format!("repos-star-cell-{row_ix}"))
                        .a11y_toggle(
                            if favorite {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            },
                            favorite,
                        )
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(30.))
                        .on_click({
                            let store = self.store.clone();
                            let folder = folder.clone();
                            move |_, _, cx| {
                                cx.stop_propagation();
                                store.update(cx, |s, cx| s.toggle_favorite(&folder, cx));
                            }
                        })
                        .child(icon(
                            "icons/icon-star.svg",
                            px(theme::icon_medium()),
                            if favorite {
                                theme::GOLD_BRIGHT
                            } else {
                                theme::STAR_MUTED
                            },
                        ));
                    let mut cell = div()
                        .relative()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(crate::ui::with_tooltip(
                            star,
                            show_tooltips,
                            if favorite {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            },
                        ));
                    // warp-agent presence: a leading dot instead of the old
                    // sparkle logo in the shortcuts cell
                    if let Some(titles) = warp_running {
                        let dot = div()
                            .id(SharedString::from(format!("warp-running-{row_ix}")))
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .flex()
                            .items_center()
                            .child(div().rounded_full().size(px(7.)).bg(theme::SUCCESS_BRIGHT));
                        cell = cell.child(crate::ui::with_tooltip(
                            dot,
                            show_tooltips,
                            SharedString::from(format!("warp-agent is running: {titles}")),
                        ));
                    }
                    cell.into_any_element()
                }
            }
            ColKind::Name => {
                if archived {
                    // limp identity; the hover-revealed restore lives in the
                    // activity cell, at the row's end
                    div()
                        .debug_selector(move || format!("repos-name-cell-{row_ix}"))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(theme::font_title()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::TEXT_FAINT)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(repo.name.clone()),
                        )
                        .into_any_element()
                } else {
                    let store = self.store.clone();
                    let notes_name = repo.name.clone();
                    let notes_button = div()
                        .id(("open-notes", row_ix))
                        .a11y_button("Open notes")
                        .cursor_pointer()
                        .flex_shrink_0()
                        .rounded_md()
                        .p_1p5()
                        .flex()
                        .items_center()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            store.update(cx, |s, _| {
                                let settings = s.settings();
                                let root = devtools_core::notes::resolve_store_root(
                                    &settings.general.notes_store_path,
                                );
                                if let Err(err) = devtools_core::warp_notes::launch(
                                    &settings.warp_notes.executable,
                                    &root,
                                    Some(&notes_name),
                                    None,
                                ) {
                                    s.notify_error(err);
                                }
                            });
                        })
                        .child(icon(
                            "icons/icon-note.svg",
                            px(theme::icon_medium()),
                            theme::TEXT_MUTED,
                        ));
                    // one semibold line — the folder path left the table when the
                    // rows went tall; the name uses the whole column
                    div()
                        .debug_selector(move || format!("repos-name-cell-{row_ix}"))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .when(notes_enabled, |cell| {
                            cell.child(crate::ui::with_tooltip(
                                notes_button,
                                show_tooltips,
                                "Open notes for this repository",
                            ))
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(theme::font_title()))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::TEXT)
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(repo.name.clone()),
                        )
                        .into_any_element()
                }
            }
            ColKind::Shortcuts => {
                if archived {
                    div().into_any_element()
                } else {
                    shortcuts_cell(repo, &settings, row_ix, &self.store)
                }
            }
            // branch pill and changes cell open the bar's Changes tab; the
            // GitHub/Azure cells route to their own tabs. Archived rows
            // render the same cells faded with no click routing.
            ColKind::Branch => {
                let cell = branch_pill(repo.git_branch_name.as_deref());
                if archived {
                    faded(cell)
                } else {
                    tab_cell(
                        ("open-changes", row_ix as u64),
                        &folder,
                        BottomBarTab::Changes,
                        &self.store,
                        cell,
                    )
                }
            }
            ColKind::Changes => {
                let cell = changes_cell(repo, layout.metrics.compact_changes);
                if archived {
                    faded(cell)
                } else {
                    tab_cell(
                        ("open-changes", row_ix as u64),
                        &folder,
                        BottomBarTab::Changes,
                        &self.store,
                        cell,
                    )
                }
            }
            ColKind::Github => {
                let cell = github_cell(repo);
                if archived {
                    faded(cell)
                } else {
                    let tab = if repo.github_pr_count > 0 {
                        BottomBarTab::PullRequests
                    } else {
                        BottomBarTab::Issues
                    };
                    let has_content = repo.github_pr_count > 0 || repo.github_issue_count > 0;
                    if has_content {
                        tab_cell(("open-gh", row_ix as u64), &folder, tab, &self.store, cell)
                    } else {
                        cell
                    }
                }
            }
            ColKind::Azure => {
                let cell = azure_cell(repo, &self.store, cx);
                if archived {
                    faded(cell)
                } else {
                    tab_cell(
                        ("open-azure", row_ix as u64),
                        &folder,
                        BottomBarTab::Azure,
                        &self.store,
                        cell,
                    )
                }
            }
            ColKind::Activity => {
                // the row's end carries its one archive control: hovering
                // the row swaps the last-activity value for it (archive on
                // live rows, restore on archived ones)
                let store = self.store.clone();
                let action_folder = folder.clone();
                let action_name = repo.name.clone();
                let action = div()
                    .id(("archive-toggle", row_ix))
                    .debug_selector(move || {
                        if archived {
                            format!("repos-restore-{row_ix}")
                        } else {
                            format!("repos-archive-toggle-{row_ix}")
                        }
                    })
                    .a11y_button(if archived {
                        "Unarchive repository"
                    } else {
                        "Archive repository"
                    })
                    .cursor_pointer()
                    .rounded_md()
                    .p_1p5()
                    .flex()
                    .items_center()
                    .opacity(0.)
                    .group_hover("repo-row", |s| s.opacity(1.))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        store.update(cx, |s, cx| {
                            if archived {
                                s.set_repo_archived(&action_folder, false, cx);
                                s.notify_success(format!("Restored {action_name}"));
                            } else {
                                s.set_repo_archived(&action_folder, true, cx);
                                s.notify_success(format!("Archived {action_name}"));
                            }
                            cx.notify();
                        });
                    })
                    .child(icon(
                        if archived {
                            "icons/icon-undo.svg"
                        } else {
                            "icons/icon-package.svg"
                        },
                        px(theme::icon_medium()),
                        theme::TEXT_MUTED,
                    ));
                let action = crate::ui::with_tooltip(
                    action,
                    show_tooltips,
                    if archived {
                        "Unarchive repository"
                    } else {
                        "Archive repository"
                    },
                );
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(
                        div()
                            .opacity(if archived { 0.55 } else { 1. })
                            .group_hover("repo-row", |s| s.opacity(0.))
                            .child(
                                repo.git_last_commit_at
                                    .map(devtools_core::rel_time::format)
                                    .unwrap_or_else(|| "—".into()),
                            ),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .items_center()
                            .child(action),
                    )
                    .into_any_element()
            }
        };

        // the kit's cell wrapper is a block div and percentage heights don't
        // resolve through it, so cell content would hug the row top; every
        // cell centers inside an explicit flex column instead. The height
        // must leave the kit's own 4+4 cell padding free — at the full row
        // height the column overflows 8px and content rides ~4px low.
        // The hover group lives on the row (render_tr), not the cell: hover
        // anywhere on the row must reach the activity cell's action.
        div()
            .debug_selector(move || format!("repos-td-wrap-{row_ix}"))
            .h(px(theme::TABLE_ROW_H - 8.))
            .flex()
            .flex_col()
            .justify_center()
            .child(cell)
            .into_any_element()
    }

    fn render_empty(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let (scanning, searching, has_folders) = {
            let store = self.store.read(cx);
            let has_folders = store
                .settings()
                .repos
                .repo_scan_folders
                .as_ref()
                .is_some_and(|f| !f.is_empty());
            (store.scanning, !store.tag_filters.is_empty(), has_folders)
        };
        let text = if scanning {
            "Scanning folders…"
        } else if searching {
            "No repositories match the current filter"
        } else if has_folders {
            "No repositories found under the scan folders"
        } else {
            "No scan folders configured — add Repos.RepoScanFolders in ~/.devtools/settings.json"
        };
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .py_8()
            .text_size(px(theme::font_body()))
            .text_color(theme::TEXT_MUTED)
            .child(text)
            .into_any_element()
    }
}

/// Wraps a cell's content for an archived row: everything renders, but limp
/// and without the interactive wrapper (the row is inert except restore).
fn faded(cell: gpui::AnyElement) -> gpui::AnyElement {
    div().opacity(0.55).child(cell).into_any_element()
}

/// The folder group header's cells: col 0 carries the collapse chevron,
/// col 1 the identity row (folder glyph, group name with the full folder as
/// tooltip, repo count, hover-revealed solo filter), the rest empty — a
/// header is one full-width band, not a row of values. Both buttons stop
/// propagation so the row never routes a selection.
fn group_header_cell(
    row: &ProjectedRow,
    row_ix: usize,
    col_ix: usize,
    store: &Entity<Store>,
    show_tooltips: bool,
) -> gpui::AnyElement {
    let ProjectedRow::Group {
        key,
        name,
        count,
        collapsed,
    } = row
    else {
        return div().into_any_element();
    };
    let other = key == crate::store::projection::OTHER_GROUP_KEY;
    let cell: gpui::AnyElement = match col_ix {
        0 => {
            let chevron = div()
                .id(("group-chevron", row_ix as u64))
                .debug_selector({
                    let selector = format!("repos-group-header-{key}-chevron");
                    move || selector.clone()
                })
                .a11y_button(if *collapsed {
                    "Expand group"
                } else {
                    "Collapse group"
                })
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .size(px(30.))
                .on_click({
                    let store = store.clone();
                    let key = key.to_string();
                    move |_, _, cx| {
                        cx.stop_propagation();
                        store.update(cx, |s, cx| s.toggle_group_collapse(&key, cx));
                    }
                })
                .child(icon(
                    if *collapsed {
                        "icons/icon-chevron-right.svg"
                    } else {
                        "icons/icon-chevron-down.svg"
                    },
                    px(theme::icon_medium()),
                    theme::TEXT_FAINT,
                ));
            crate::ui::with_tooltip(
                chevron,
                show_tooltips,
                if *collapsed {
                    "Expand group"
                } else {
                    "Collapse group"
                },
            )
            .into_any_element()
        }
        1 => {
            // the folder icon doubles as the "show only this group" button:
            // hovering anywhere on the header row swaps it to the search
            // action in place (the reveal group is the row's, render_tr)
            let solo_swap = div()
                .id(("group-solo", row_ix as u64))
                .debug_selector({
                    let selector = format!("repos-group-header-{key}-solo");
                    move || selector.clone()
                })
                .a11y_button("Show only this group")
                .relative()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .size(px(theme::icon_medium()))
                .rounded_md()
                .hover(|s| s.bg(theme::HOVER))
                .on_click({
                    let store = store.clone();
                    let key = key.to_string();
                    move |_, _, cx| {
                        cx.stop_propagation();
                        store.update(cx, |s, cx| s.solo_group(&key, cx));
                    }
                })
                .child(
                    icon(
                        "icons/icon-folder.svg",
                        px(theme::icon_medium()),
                        theme::TEXT_FAINT,
                    )
                    .group_hover("repos-group-header", |s| s.opacity(0.)),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            icon(
                                "icons/icon-search.svg",
                                px(theme::icon_medium()),
                                theme::TEXT_MUTED,
                            )
                            .opacity(0.)
                            .group_hover("repos-group-header", |s| s.opacity(1.)),
                        ),
                );
            let solo_swap =
                crate::ui::with_tooltip(solo_swap, show_tooltips, "Show only this group");
            let name_label = crate::ui::with_tooltip(
                div()
                    .id(("group-name", row_ix as u64))
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(theme::font_title() - 2.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme::TEXT)
                    .child(name.to_string()),
                show_tooltips,
                if other {
                    "Repositories under no scan root".to_string()
                } else {
                    format!("Folder {key}")
                },
            );
            div()
                .debug_selector({
                    let selector = format!("repos-group-header-{key}-name");
                    move || selector.clone()
                })
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(solo_swap)
                .child(name_label)
                .child(
                    div()
                        .debug_selector({
                            let selector = format!("repos-group-header-{key}-count");
                            move || selector.clone()
                        })
                        .rounded_full()
                        .bg(theme::CHIP)
                        .px_1p5()
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_MUTED)
                        .child(format!("{count}")),
                )
                .into_any_element()
        }
        _ => div().into_any_element(),
    };
    div()
        .debug_selector(move || format!("repos-td-wrap-{row_ix}"))
        .h(px(theme::TABLE_ROW_H - 8.))
        .flex()
        .flex_col()
        .justify_center()
        .child(cell)
        .into_any_element()
}

/// Wraps a table cell so clicking it opens the bottom bar on one specific
/// tab without re-firing the row toggle.
fn tab_cell(
    id: (&'static str, u64),
    folder: &str,
    tab: BottomBarTab,
    store: &Entity<Store>,
    cell: gpui::AnyElement,
) -> gpui::AnyElement {
    let folder = folder.to_string();
    let store = store.clone();
    div()
        .id(gpui::ElementId::NamedInteger(id.0.into(), id.1))
        .a11y_button(format!("{folder} — open {}", tab.label()))
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let folder = folder.clone();
            store.update(cx, |s, cx| s.open_repo_tab(&folder, tab, cx));
        })
        .child(cell)
        .into_any_element()
}

fn branch_pill(branch: Option<&str>) -> gpui::AnyElement {
    let Some(branch) = branch.filter(|b| !b.is_empty()) else {
        return div()
            .text_color(theme::TEXT_FAINT)
            .child("—")
            .into_any_element();
    };
    let color = if branch == "main" {
        theme::SUCCESS_BRIGHT
    } else if branch == "master" {
        theme::PURPLE_BRIGHT
    } else if branch.starts_with("develop") {
        theme::BLUE_BRIGHT
    } else if branch.starts_with("release") {
        theme::AMBER_BRIGHT
    } else {
        theme::TEXT_MUTED
    };
    let pill_selector = format!("branch-pill-{branch}");
    div()
        .debug_selector(move || pill_selector.clone())
        .rounded_md()
        .bg(theme::tint(color))
        .px_2()
        .py_0p5()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .text_size(px(theme::font_small()))
        .font_family(gpui::SharedString::from(theme::code_font()))
        .text_color(color)
        .child(icon(
            "icons/icon-source-branch.svg",
            px(theme::icon_small() - 2.),
            color,
        ))
        .child(crate::ui::trim_branch_label(
            branch,
            crate::ui::BRANCH_LABEL_CHARS,
        ))
        .into_any_element()
}

/// The changes cell: an amber count + "changes" when the tree is dirty, the
/// GitHub column's quiet "—" when it is clean, and push/pull arrows only when
/// there is actually something to sync.
fn changes_cell(repo: &RepoEntry, compact: bool) -> gpui::AnyElement {
    let synced = repo.git_to_push_count == 0 && repo.git_to_pull_count == 0;
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .text_size(px(theme::font_small()));
    if repo.git_modified_count > 0 {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::AMBER_BRIGHT))
                .px_1p5()
                .py_0p5()
                .text_size(px(theme::font_small()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::AMBER_BRIGHT)
                .child(format!("{}", repo.git_modified_count)),
        );
        if !compact {
            row = row.child(div().text_color(theme::TEXT_DIM).child("changes"));
        }
    } else if synced {
        // a fully settled repo has nothing to say — the GitHub column's dash
        row = row.child(
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_FAINT)
                .child("—"),
        );
    }
    if !synced {
        let arrow = |glyph: &'static str, count: u32, color: gpui::Rgba| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_0p5()
                .child(icon(glyph, px(theme::icon_small() - 4.), color))
                .child(div().text_color(color).child(format!("{count}")))
        };
        if repo.git_to_push_count > 0 {
            row = row.child(arrow(
                "icons/icon-arrow-up.svg",
                repo.git_to_push_count,
                theme::SUCCESS_BRIGHT,
            ));
        }
        if repo.git_to_pull_count > 0 {
            row = row.child(arrow(
                "icons/icon-arrow-down.svg",
                repo.git_to_pull_count,
                theme::BLUE_BRIGHT,
            ));
        }
    }
    row.into_any_element()
}

/// The GitHub chips: purple (pull requests) and green (issues) counts under
/// the .NET row glyphs, or the gray check once the pass loaded a GitHub repo
/// with none.
fn github_cell(repo: &RepoEntry) -> gpui::AnyElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .text_size(px(theme::font_small()));
    if repo.github_pr_count > 0 {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::PURPLE_BRIGHT))
                .px_1p5()
                .py_0p5()
                .flex()
                .flex_row()
                .items_center()
                .gap_0p5()
                .text_color(theme::PURPLE_BRIGHT)
                .child(icon(
                    "icons/icon-git-pull-request.svg",
                    px(theme::icon_small() - 3.),
                    theme::PURPLE_BRIGHT,
                ))
                .child(format!("{}", repo.github_pr_count)),
        );
    }
    if repo.github_issue_count > 0 {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::SUCCESS_BRIGHT))
                .px_1p5()
                .py_0p5()
                .flex()
                .flex_row()
                .items_center()
                .gap_0p5()
                .text_color(theme::SUCCESS_BRIGHT)
                .child(icon(
                    "icons/icon-issue.svg",
                    px(theme::icon_small() - 3.),
                    theme::SUCCESS_BRIGHT,
                ))
                .child(format!("{}", repo.github_issue_count)),
        );
    }
    if repo.github_pr_count == 0 && repo.github_issue_count == 0 {
        row = row.child(
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_FAINT)
                .child("—"),
        );
    }
    row.into_any_element()
}

fn azure_cell(repo: &RepoEntry, store: &Entity<Store>, cx: &App) -> gpui::AnyElement {
    let folder = repo.folder_path.to_string_lossy().to_string();
    let Some(snapshot) = store.read(cx).azure_by_folder.get(&folder) else {
        return div()
            .text_size(px(theme::font_small()))
            .text_color(theme::TEXT_FAINT)
            .child("·")
            .into_any_element();
    };
    if snapshot.pipelines.is_empty() && snapshot.pulls.is_empty() && snapshot.work_items.is_empty()
    {
        return div()
            .text_size(px(theme::font_small()))
            .text_color(theme::TEXT_FAINT)
            .child("·")
            .into_any_element();
    }
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_1()
        .text_size(px(theme::font_small()));
    // the newest run's state is the repo's pipeline state (.NET cell glyphs:
    // check = ok, close = failed, play = running)
    if !snapshot.pipelines.is_empty() {
        let (glyph, color) = if snapshot.pipeline_running() {
            ("icons/icon-play.svg", theme::BLUE_BRIGHT)
        } else if snapshot.pipeline_failed() {
            ("icons/icon-close.svg", theme::DANGER)
        } else if snapshot.pipeline_ok() {
            ("icons/icon-check.svg", theme::SUCCESS_BRIGHT)
        } else {
            ("icons/icon-clock.svg", theme::TEXT_MUTED)
        };
        row = row.child(icon(glyph, px(theme::icon_small() - 3.), color));
    }
    if !snapshot.pulls.is_empty() {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::PURPLE_BRIGHT))
                .px_1p5()
                .text_color(theme::PURPLE_BRIGHT)
                .child(format!("PR {}", snapshot.pulls.len())),
        );
    }
    if !snapshot.work_items.is_empty() {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::AMBER_BRIGHT))
                .px_1p5()
                .text_color(theme::AMBER_BRIGHT)
                .child(format!("WI {}", snapshot.work_items.len())),
        );
    }
    row.into_any_element()
}

/// Per-row buttons: bordered squares, launch order
/// [opencode][deepseek][solution][vscode][folder][terminal]. Purely visibility switches from
/// settings — a configured-but-missing executable reports at launch time.
fn shortcuts_cell(
    repo: &RepoEntry,
    settings: &AppSettings,
    row_ix: usize,
    store: &Entity<Store>,
) -> gpui::AnyElement {
    use devtools_core::terminal;
    let folder = repo.folder_path.to_string_lossy().to_string();
    let show_tooltips = settings.general.show_tooltips;
    let mut row = div().flex().flex_row().items_center().gap_1p5();

    // the agent door: opens the repo as a warp-agent workspace (added when
    // missing, launched when closed)
    if settings.warp_agent.enable_warp_agent {
        let store = store.clone();
        let folder_for_open = folder.clone();
        let element = div()
            .id(SharedString::from(format!("open-warp-{row_ix}")))
            .debug_selector(move || format!("repos-warp-cell-{row_ix}"))
            .a11y_button("Open in warp-agent")
            .cursor_pointer()
            .rounded_md()
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .hover(|s| s.bg(theme::HOVER))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                let folder = folder_for_open.clone();
                store.update(cx, |s, cx| s.open_in_warp(folder, cx));
            })
            .child(icon(
                "icons/icon-agent-sparkle.svg",
                px(theme::icon_large()),
                theme::ACCENT,
            ));
        row = row.child(crate::ui::with_tooltip(
            element,
            show_tooltips,
            "Open in warp-agent",
        ));
    }

    let button = |id: String,
                  icon_path: &'static str,
                  tint_color: gpui::Rgba,
                  tip: &'static str,
                  launch: Box<dyn Fn() -> Result<(), String>>| {
        let store = store.clone();
        let element = div()
            .id(SharedString::from(id))
            .a11y_button(tip)
            .cursor_pointer()
            .rounded_md()
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .hover(|s| s.bg(theme::HOVER))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                if let Err(err) = launch() {
                    store.update(cx, |s, cx| {
                        s.notify_error(err);
                        cx.notify();
                    });
                }
            })
            .child(icon(icon_path, px(theme::icon_large()), tint_color));
        crate::ui::with_tooltip(element, show_tooltips, tip)
    };

    // CLI quick-launch group: OpenCode, then DeepSeek — one instance per
    // enabled CLI in the repo. The selected model belongs to the AI harness,
    // so opencode only launches with it when that harness is opencode; the
    // dsh CLI boots its own profile and takes no model.
    let harness = settings.warp_agent.selected_harness().to_string();
    let model = settings.opencode.model.trim().to_string();
    let term = settings.repos.terminal_executable.clone();
    let cli_launch = |exe_setting: &str,
                      fallback: &'static str,
                      name: &'static str,
                      pass_model: bool|
     -> Box<dyn Fn() -> Result<(), String>> {
        let exe = if exe_setting.trim().is_empty() {
            fallback.to_string()
        } else {
            exe_setting.trim().to_string()
        };
        if pass_model && harness == fallback {
            if model.is_empty() {
                Box::new(move || Err(format!("No {name} model — set it in Settings")))
            } else {
                Box::new({
                    let folder = folder.clone();
                    let exe = exe.clone();
                    let model = model.clone();
                    let term = term.clone();
                    move || {
                        terminal::launch_argv_in_terminal(
                            &folder,
                            &[exe.clone(), "--model".into(), model.clone()],
                            &term,
                        )
                    }
                })
            }
        } else {
            Box::new({
                let folder = folder.clone();
                let exe = exe.clone();
                let term = term.clone();
                move || {
                    terminal::launch_argv_in_terminal(&folder, std::slice::from_ref(&exe), &term)
                }
            })
        }
    };
    let mut cli_buttons = 0usize;
    if settings.opencode.enable_opencode {
        row = row.child(button(
            format!("open-opencode-{row_ix}"),
            "icons/icon-opencode.svg",
            theme::TEXT_MUTED,
            "Launch OpenCode with the selected model",
            cli_launch(
                &settings.repos.opencode_executable,
                "opencode",
                "OpenCode",
                true,
            ),
        ));
        cli_buttons += 1;
    }
    if settings.deepseek.enable_deepseek {
        row = row.child(button(
            format!("open-deepseek-{row_ix}"),
            "icons/icon-deepseek.svg",
            theme::BLUE_BRIGHT,
            "Launch DeepSeek",
            cli_launch(
                &settings.repos.deepseek_executable,
                "dsh",
                "DeepSeek",
                false,
            ),
        ));
        cli_buttons += 1;
    }
    if cli_buttons > 0 {
        // .NET separates the CLI group from the launch buttons
        row = row.child(
            div()
                .mx_1()
                .self_center()
                .h(px(20.))
                .w(px(1.))
                .bg(theme::HAIRLINE),
        );
    }

    // Open-solution slot: Visual Studio exists only on Windows, so the whole
    // slot — button or empty reservation — drops on other platforms; the
    // reservation keeps repos without a solution aligned with the rest.
    if cfg!(windows) && settings.repos.enable_visual_studio {
        match repo.solution_path.clone() {
            Some(solution) => {
                let ide = settings.repos.ide_executable.clone();
                row = row.child(button(
                    format!("open-solution-{row_ix}"),
                    "icons/icon-sln-box.svg",
                    theme::PURPLE_BRIGHT,
                    "Open solution in Visual Studio",
                    Box::new(move || terminal::open_solution(&solution, &ide)),
                ));
            }
            None => row = row.child(div().size(px(36.))),
        }
    }
    if settings.repos.enable_vscode {
        row = row.child(button(
            format!("open-vscode-{row_ix}"),
            "icons/icon-vscode.svg",
            theme::BLUE_BRIGHT,
            "Open in VS Code",
            Box::new({
                let folder = folder.clone();
                let exe = settings.repos.vscode_executable.clone();
                let profile = settings.repos.vscode_profile.clone();
                move || terminal::open_in_vscode(&folder, &exe, &profile)
            }),
        ));
    }
    row = row.child(button(
        format!("open-folder-{row_ix}"),
        "icons/icon-folder.svg",
        theme::AMBER_BRIGHT,
        "Open folder",
        Box::new({
            let folder = folder.clone();
            move || terminal::open_folder(&folder)
        }),
    ));
    if settings.repos.enable_terminal {
        row = row.child(button(
            format!("open-terminal-{row_ix}"),
            "icons/icon-terminal.svg",
            theme::SUCCESS_BRIGHT,
            "Open terminal",
            Box::new({
                let folder = folder.clone();
                let term = settings.repos.terminal_executable.clone();
                move || terminal::open_terminal(&folder, &term)
            }),
        ));
    }
    row.into_any_element()
}

/// The yesterday-summary pill. Lives in the titlebar next to the app name
/// (rendered by the window), so it works on every page.
pub(crate) fn summary_button(store: &Entity<Store>, busy: bool) -> gpui::AnyElement {
    let handle = store.clone();
    div()
        .id("summary-button")
        .debug_selector(|| "summary-button".into())
        .a11y_button(if busy {
            "Summarizing…"
        } else {
            "Daily summary"
        })
        .cursor_pointer()
        // the pill rides in the titlebar's drag region
        .occlude()
        .rounded_md()
        .h(px(30.))
        .px_2p5()
        .flex()
        .items_center()
        .gap_1p5()
        .text_size(px(theme::font_small()))
        .text_decoration_none()
        .when(busy, |d| d.text_color(theme::ACCENT))
        .when(!busy, |d| d.text_color(theme::TEXT_DIM))
        .bg(theme::CHIP)
        .hover(|s| s.opacity(0.9))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click({
            let store = handle.clone();
            move |_, _, cx| {
                store.update(cx, |s, cx| s.run_summary(cx));
            }
        })
        .child(icon(
            "icons/icon-agent-sparkle.svg",
            px(theme::icon_small() - 1.),
            if busy { theme::ACCENT } else { theme::TEXT_DIM },
        ))
        .child(if busy {
            "summarizing…"
        } else {
            "Daily summary"
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(launchers: &[&str]) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.repos.enable_terminal = launchers.contains(&"terminal");
        settings.repos.enable_vscode = launchers.contains(&"vscode");
        settings.repos.enable_visual_studio = launchers.contains(&"sln");
        settings.opencode.enable_opencode = launchers.contains(&"opencode");
        settings.deepseek.enable_deepseek = launchers.contains(&"deepseek");
        settings.warp_agent.enable_warp_agent = launchers.contains(&"warp");
        settings
    }

    #[test]
    fn shortcuts_column_hugs_the_enabled_launchers() {
        // folder only: one 36px button, no gaps, no divider, slack 8
        assert_eq!(shortcuts_column_width(&settings_with(&[])), px(44.));
        // folder + terminal: two buttons + one gap
        assert_eq!(
            shortcuts_column_width(&settings_with(&["terminal"])),
            px(86.)
        );
        // deepseek alone keeps the CLI group's divider: folder + deepseek +
        // divider = 3 children -> 2 gaps + divider (1px + mx_1 = 9)
        assert_eq!(
            shortcuts_column_width(&settings_with(&["deepseek"])),
            px(2. * 36. + 2. * 6. + 9. + 8.)
        );
        // full linux set: warp door, opencode + deepseek behind one divider,
        // vscode, folder, terminal = 6 buttons + divider = 7 children -> 6
        // gaps + 9; the solution slot only joins on Windows
        let full = shortcuts_column_width(&settings_with(&[
            "terminal", "vscode", "opencode", "deepseek", "warp",
        ]));
        assert_eq!(full, px(6. * 36. + 6. * 6. + 9. + 8.));
    }

    /// Selection can originate outside a row click (global search, the notes
    /// button, E2E hooks). The table must mirror it into its highlight without
    /// the queued SelectRow event looping back through `select_repo` and
    /// toggling the already-selected row closed.
    #[gpui::test]
    fn external_selection_mirrors_into_the_table(cx: &mut gpui::TestAppContext) {
        cx.update(gpui_kit::init);
        cx.update(theme::apply_kit_theme);

        let home = std::env::temp_dir().join(format!("devtools-repos-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join(".devtools")).unwrap();
        // SAFETY: process-global env, set before Store::new; the other tests in
        // this binary don't read HOME.
        unsafe { std::env::set_var("HOME", &home) };
        // the table's selection starts empty; the split layout selects a row
        unsafe { std::env::set_var("DEVTOOLS_UI_TABLE", "1") };

        let entry = |name: &str, path: &str| devtools_core::repo_store::RepoEntry {
            name: name.into(),
            folder_path: std::path::PathBuf::from(path),
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
        };

        let (net_tx, _net_rx) = async_channel::unbounded();
        let store = cx.new(|cx| {
            let mut store = Store::new(cx, net_tx);
            // pin the flat layout: the live settings must not re-shape the rows
            store.group_by_root = false;
            store.replace_repos(vec![
                entry("alpha", "/probe/alpha"),
                entry("beta", "/probe/beta"),
            ]);
            store
        });
        let store_for_page = store.clone();
        let (page, cx) =
            cx.add_window_view(|window, cx| ReposPage::new(store_for_page, window, cx));
        cx.run_until_parked();
        assert_eq!(
            cx.read_entity(&page, |page, cx| page.table.read(cx).selected_row()),
            None,
            "no row is highlighted before a selection"
        );

        cx.update_entity(&store, |s, cx| {
            s.select_repo(Some("/probe/beta".into()), cx)
        });
        cx.run_until_parked();
        assert_eq!(
            cx.read_entity(&store, |s, _| s.selected_repo.clone())
                .as_deref(),
            Some("/probe/beta"),
            "mirroring the selection must not toggle the bar closed"
        );
        assert_eq!(
            cx.read_entity(&page, |page, cx| page.table.read(cx).selected_row()),
            Some(1),
            "the table highlights the externally selected row"
        );

        cx.update_entity(&store, |s, cx| s.close_bar(cx));
        cx.run_until_parked();
        assert_eq!(
            cx.read_entity(&page, |page, cx| page.table.read(cx).selected_row()),
            None,
            "closing the bar clears the table highlight"
        );

        let _ = std::fs::remove_dir_all(&home);
    }
}
