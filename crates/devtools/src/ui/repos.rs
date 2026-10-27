//! The Repos page: toolbar (tag chips, refresh, totals) and the repos table.
//! The table delegate reads the store's projection at render time, so
//! filter/sort/status changes land with the next frame — no mirrored
//! row list to keep in sync. Sorting is driven entirely by the table's
//! sortable headers.

use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::dialogs::{AddRepoDialog, CloneDialog, Dialog};
use crate::ui::icon;
use devtools_core::repo_store::RepoEntry;
use devtools_core::settings::{AppSettings, RepoSortMode};
use gpui::prelude::*;
use gpui::{div, px, App, Context, Entity, FontWeight, Pixels, SharedString, Window};
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

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
                let key = (store.projection_generation(), store.selected_repo.clone().unwrap_or_default());
                if key == *last_sync.borrow() {
                    Sync::Skip
                } else {
                    *last_sync.borrow_mut() = key.clone();
                    Sync::Row(store.selected_repo.as_deref().and_then(|folder| {
                        table_for_observe.read(cx).delegate().row_for_folder(folder, cx)
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
                        table_for_observe.update(cx, |table, cx| {
                            table.set_selected_row(row_ix, cx)
                        });
                    }
                    None => table_for_observe.update(cx, |table, cx| table.clear_selection(cx)),
                }
            }
            cx.notify();
        })
        .detach();
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
        }
    }
}

impl ReposPage {
    /// Escape closes the topmost page overlay. Returns whether one was
    /// actually open, so the window can fall through to its own layers.
    pub fn dismiss_overlay(&mut self, cx: &mut Context<Self>) -> bool {
        if self.dialog.is_some() {
            self.dialog = None;
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
        let Some(Dialog::AddRepo(dialog)) = &mut self.dialog else { return };
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
        let _ = self.store.read(cx).net.send_blocking(crate::net::NetCommand::DeepScan {
            root: root.trim().to_string(),
            depth,
            settings: settings.repos.clone(),
        });
        cx.notify();
    }

    /// Adds every checked entry from the Add-Repositories dialog.
    pub fn commit_add_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::AddRepo(dialog)) = &mut self.dialog else { return };
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
        self.store.update(cx, |store, cx| store.register_repos(&selected, cx));
        self.dialog = None;
        cx.notify();
    }

    /// Sends the Clone dialog's clone (with inline validation).
    pub fn commit_clone(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Clone(dialog)) = &mut self.dialog else { return };
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
            } else if destination.trim().is_empty() || !std::path::Path::new(&destination).is_dir() {
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
        let _ = self.store.read(cx).net.send_blocking(crate::net::NetCommand::Clone {
            url: url.trim().to_string(),
            parent_dir: destination,
            name,
        });
        cx.notify();
    }
}

impl Render for ReposPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // drain dialog-bound results the moment we are about to draw
        let drained_deep_scan = self.store.update(cx, |store, _| store.deep_scan_result.take());
        if let Some(found) = drained_deep_scan
            && let Some(Dialog::AddRepo(dialog)) = &mut self.dialog {
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
        if let Some(path) = self.store.update(cx, |store, _| store.folder_pick_result.take())
            && let Some(Dialog::AddRepo(dialog)) = &mut self.dialog
        {
            dialog.root.update(cx, |field, cx| field.set_value(&path, window, cx));
        }
        let drained_clone = self.store.update(cx, |store, _| store.clone_result.take());
        if let Some((parent_dir, name, result)) = drained_clone {
            if result.success {
                self.store.update(cx, |store, cx| store.adopt_clone(&parent_dir, &name, cx));
                self.dialog = None;
            } else if let Some(Dialog::Clone(dialog)) = &mut self.dialog {
                dialog.busy = false;
                dialog.error = result.error.or_else(|| Some("clone failed".into()));
            }
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
                Rc::new(TableLayout { key, cols, metrics, name, settings })
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
        page
    }
}

impl ReposPage {
    fn render_toolbar(&mut self, viewport_width: Pixels, cx: &mut Context<Self>) -> gpui::AnyElement {
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

        let (total_repos, total_changes, total_prs, total_issues, github_on, filters_active) = {
            let store = self.store.read(cx);
            // totals describe the live list: archived repos leave every sum
            let live = store.repos.iter().filter(|r| !r.is_archived());
            (
                live.clone().count(),
                live.clone().map(|r| r.git_modified_count).sum::<u32>(),
                live.clone().map(|r| r.github_pr_count).sum::<u32>(),
                live.map(|r| r.github_issue_count).sum::<u32>(),
                store.settings().repos.enable_git_hub,
                !store.tag_filters.is_empty(),
            )
        };

        // The header runs out of room before the filter row does; the count
        // chips are the first thing to go (the table carries the same numbers
        // per row). The threshold scales with the text size so a large font
        // does not clip the action buttons. The chips also vary with the data
        // (remote counts arrive after the first frame), so they drop in
        // priority order: remote counts first, then the local changes total —
        // never the action buttons.
        let scale = theme::font_small() / 14.0;
        let compact = viewport_width < px(840.) * scale;
        let roomy = viewport_width >= px(1000.) * scale;
        let show_change_chip = !compact;
        let show_remote_chips = show_change_chip && roomy;

        let count_chip = |count: u32,
                          label: &'static str,
                          color: gpui::Rgba,
                          icon_path: Option<&'static str>| {
            div()
                .rounded_full()
                .px_2()
                .py_0p5()
                .text_size(px(theme::font_small() - 1.))
                .font_weight(FontWeight::MEDIUM)
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .bg(theme::tint(color))
                .text_color(color)
                .children(icon_path.map(|path| icon(path, px(theme::icon_small() - 3.), color)))
                .child(format!("{count} {label}"))
        };

        // tag filters read as pills: tinted+bordered when active, quiet
        // outlines otherwise (OR semantics across the checked set)
        let mut tag_pills = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1p5()
            .flex_1()
            .min_w_0()
            .overflow_hidden();
        for (index, tag) in tags.iter().enumerate() {
            let active = active_tags.iter().any(|t| t.eq_ignore_ascii_case(tag));
            tag_pills = tag_pills.child(
                div()
                    .id(gpui::ElementId::NamedInteger("filter".into(), index as u64))
                    .cursor_pointer()
                    .rounded_full()
                    .border_1()
                    .border_color(if active { theme::ACCENT } else { theme::HAIRLINE })
                    .px_2()
                    .py_0p5()
                    .text_size(px(theme::font_small() - 1.))
                    .when(active, |d| {
                        d.bg(theme::tint(theme::ACCENT))
                            .text_color(theme::ACCENT)
                            .font_weight(FontWeight::MEDIUM)
                    })
                    .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                    .hover(|s| s.border_color(theme::TEXT_FAINT).text_color(theme::TEXT))
                    .on_click({
                        let tag = tag.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.store.update(cx, |s, cx| s.toggle_tag_filter(&tag, cx));
                        })
                    })
                    .child(tag.clone()),
            );
        }

        let store_refresh = self.store.clone();
        let header_row = div()
            .h(px(theme::TOOLBAR_H))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_4()
            .child(icon("icons/icon-logo-mark.svg", px(theme::icon_medium()), theme::TEXT_FAINT))
            .child(
                div()
                    .text_size(px(theme::font_title() + 1.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::TEXT)
                    .child("Repositories"),
            )
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(format!("{total_repos} repos")),
            )
            .when(show_change_chip && total_changes > 0, |row| {
                row.child(count_chip(total_changes, "changes", theme::AMBER_BRIGHT, None))
            })
            .when(show_remote_chips && github_on && total_prs > 0, |row| {
                row.child(count_chip(
                    total_prs,
                    "PRs",
                    theme::PURPLE_BRIGHT,
                    Some("icons/icon-git-pull-request.svg"),
                ))
            })
            .when(show_remote_chips && github_on && total_issues > 0, |row| {
                row.child(count_chip(
                    total_issues,
                    "issues",
                    theme::SUCCESS_BRIGHT,
                    Some("icons/icon-issue.svg"),
                ))
            })
            .child(div().flex_1())
            .child(icon_action(
                "refresh-button",
                "icons/icon-refresh.svg",
                if scanning { theme::ACCENT } else { theme::TEXT_MUTED },
                move |_, _, cx| {
                    store_refresh.update(cx, |s, cx| s.refresh(cx));
                },
            ))
            .child(toolbar_action(
                "add-repo-button",
                "Add Repository",
                Some("icons/icon-plus.svg"),
                true,
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

        let store_clear = self.store.clone();
        // the archived toggle rides the filter row (it is a view filter, and
        // this row never compacts away): present whenever archived repos
        // exist or are being shown, pill-styled like the tag chips
        let archive_chip_visible = archived_count > 0 || show_archived;
        let archive_chip = div()
            .id("archive-chip")
            .debug_selector(|| "archive-chip".into())
            .cursor_pointer()
            .rounded_full()
            .border_1()
            .border_color(if show_archived { theme::ACCENT } else { theme::HAIRLINE })
            .px_2()
            .py_0p5()
            .text_size(px(theme::font_small() - 1.))
            .when(show_archived, |d| {
                d.bg(theme::tint(theme::ACCENT))
                    .text_color(theme::ACCENT)
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!show_archived, |d| d.text_color(theme::TEXT_MUTED))
            .hover(|s| s.border_color(theme::TEXT_FAINT).text_color(theme::TEXT))
            .on_click(cx.listener(|this, _, _, cx| {
                this.store.update(cx, |s, cx| s.toggle_show_archived(cx));
            }))
            .child(format!("Archive ({archived_count})"));
        let filter_row = div()
            .h(px(theme::TOOLBAR_H))
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_4()
            .border_t_1()
            .border_color(theme::BORDER_LIGHT)
            .when(archive_chip_visible, |row| row.child(archive_chip))
            .child(tag_pills)
            .when(filters_active, |row| {
                row.child(
                    div()
                        .id("clear-button")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_MUTED)
                        .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                        .on_click(move |_, _, cx| {
                            store_clear.update(cx, |s, cx| s.clear_filters(cx));
                        })
                        .child("Clear"),
                )
            });

        div()
            .flex()
            .flex_col()
            .relative()
            .child(header_row)
            .child(filter_row)
            .into_any_element()
    }
}

/// A labelled toolbar button: filled when primary, outlined otherwise.
fn toolbar_action(
    id: &'static str,
    label: &'static str,
    icon_path: Option<&'static str>,
    primary: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    let color = if primary { theme::ON_ACCENT } else { theme::TEXT_DIM };
    div()
        .id(id)
        .cursor_pointer()
        .rounded_md()
        .h(px(30.))
        .px_3()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .text_size(px(theme::font_small()))
        .text_color(color)
        .debug_selector(move || id.into())
        .when(primary, |d| d.bg(theme::ACCENT).hover(|s| s.opacity(0.9)))
        .when(!primary, |d| {
            d.border_1()
                .border_color(theme::HAIRLINE)
                .hover(|s| s.bg(theme::HOVER).border_color(theme::TEXT_FAINT))
        })
        .on_click(on_click)
        .children(icon_path.map(|path| icon(path, px(theme::icon_small()), color)))
        .child(label)
        .into_any_element()
}

/// A square, borderless icon button for the toolbar's utility actions.
fn icon_action(
    id: &'static str,
    icon_path: &'static str,
    color: gpui::Rgba,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(id)
        .cursor_pointer()
        .rounded_md()
        .size(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(theme::HOVER))
        .on_click(on_click)
        .child(icon(icon_path, px(theme::icon_small()), color))
        .into_any_element()
}

/// The shortcuts column hugs its button row: 36px buttons, 6px gaps, and the
/// opencode group's divider (1px + mx_1). Follows the same settings gates as
/// `shortcuts_cell`.
fn shortcuts_column_width(settings: &AppSettings) -> Pixels {
    let mut buttons = 1usize; // open folder is always present
    if settings.repos.enable_terminal {
        buttons += 1;
    }
    if settings.repos.enable_vscode {
        buttons += 1;
    }
    if settings.repos.enable_zcode {
        buttons += 1;
    }
    if cfg!(windows) && settings.repos.enable_visual_studio {
        buttons += 1;
    }
    if settings.warp_agent.enable_warp_agent {
        buttons += 1; // the warp-agent door
    }
    let divider = settings.opencode.enable_opencode;
    if divider {
        buttons += 1; // the opencode button itself
    }
    let children = buttons + divider as usize;
    let gaps = children.saturating_sub(1) as f32 * 6.0;
    px(buttons as f32 * 36.0 + if divider { 9.0 } else { 0.0 } + gaps + 8.0)
}

/// Branch column width from the widest branch pill: git glyph, pill padding,
/// ~7.8px per character in the code font, clamped to sane bounds. The widest
/// branch is precomputed on reproject (Store::widest_branch_chars).
fn branch_column_width(store: &Entity<Store>, cx: &App) -> Pixels {
    let chars = store.read(cx).widest_branch_chars as f32;
    px(56. + chars * 7.8).clamp(px(110.), px(240.))
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
    let github = if settings.repos.enable_git_hub { px(96.) * scale } else { px(0.) };
    let azure = if settings.repos.enable_azure_dev_ops { px(120.) * scale } else { px(0.) };
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
            .projected
            .get(row_ix)
            .and_then(|&ix| store.repos.get(ix))
            .cloned()
    }

    fn folder_at(&self, row_ix: usize, cx: &App) -> Option<String> {
        self.repo_at(row_ix, cx)
            .map(|r| r.folder_path.to_string_lossy().to_string())
    }

    /// Inverse of [`Self::folder_at`]: the visible row for a folder, used to
    /// mirror the store's selection into the table highlight.
    fn row_for_folder(&self, folder: &str, cx: &App) -> Option<usize> {
        let store = self.store.read(cx);
        let folder = std::path::Path::new(folder);
        store
            .projected
            .iter()
            .position(|&ix| store.repos.get(ix).is_some_and(|repo| repo.folder_path == folder))
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
        let layout = Rc::new(TableLayout { key, cols, metrics, name, settings });
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
        match layout.cols.get(col_ix).copied().unwrap_or(ColKind::Shortcuts) {
            ColKind::Star => {
                Column::new("star", "").width(px(36.)).resizable(false).text_center()
            }
            ColKind::Name => Column::new("name", "Repository").width(layout.name).min_width(px(140.)).sort(kit_sort(RepoSortMode::Name)),
            ColKind::Branch => Column::new("branch", "Branch").width(layout.metrics.branch).resizable(false).sort(kit_sort(RepoSortMode::Branch)),
            ColKind::Changes => Column::new("changes", "Changes").width(layout.metrics.changes).resizable(false).sort(kit_sort(RepoSortMode::Changes)),
            ColKind::Github => Column::new("github", "GitHub").width(px(96.) * scale).sort(kit_sort(RepoSortMode::GitHub)),
            ColKind::Azure => {
                Column::new("azure", "Azure DevOps").width(px(120.) * scale).text_center().resizable(false)
            }
            ColKind::Activity => {
                Column::new("activity", "Last activity").width(px(84.) * scale).resizable(false).sort(kit_sort(RepoSortMode::LastActivity))
            }
            ColKind::Shortcuts => {
                // hugs its button row: the width follows the enabled launchers
                Column::new("shortcuts", "").width(shortcuts_column_width(&layout.settings)).resizable(false)
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
        let Some(repo) = store
            .projected
            .get(row_ix)
            .and_then(|&ix| store.repos.get(ix))
        else {
            return div().into_any_element();
        };
        let folder = repo.folder_path.to_string_lossy().to_string();
        let show_tooltips = settings.general.show_tooltips;
        let archived = repo.is_archived();

        let cell: gpui::AnyElement = match layout.cols.get(col_ix).copied().unwrap_or(ColKind::Shortcuts) {
            ColKind::Star => {
                if archived {
                    // archived rows are inert: no star, no toggle
                    div().into_any_element()
                } else {
                    let favorite = repo.is_favorite();
                    let star = div()
                        .id(("star", row_ix))
                        .debug_selector(move || format!("repos-star-cell-{row_ix}"))
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
                            if favorite { theme::GOLD_BRIGHT } else { theme::STAR_MUTED },
                        ));
                    crate::ui::with_tooltip(
                        star,
                        show_tooltips,
                        if favorite { "Remove from favorites" } else { "Add to favorites" },
                    )
                    .into_any_element()
                }
            }
            ColKind::Name => {
                if archived {
                    // limp identity plus the hover-revealed restore action —
                    // the only live control on the row
                    let store = self.store.clone();
                    let folder_restore = folder.clone();
                    let restore_name = repo.name.clone();
                    let restore = div()
                        .id(("restore", row_ix))
                        .debug_selector(move || format!("repos-restore-{row_ix}"))
                        .cursor_pointer()
                        .flex_shrink_0()
                        .rounded_md()
                        .p_1p5()
                        .flex()
                        .items_center()
                        .opacity(0.)
                        .group_hover("archived-row", |s| s.opacity(1.))
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            store.update(cx, |s, cx| {
                                s.set_repo_archived(&folder_restore, false, cx);
                                s.notify_success(format!("Restored {restore_name}"));
                                cx.notify();
                            });
                        })
                        .child(icon(
                            "icons/icon-undo.svg",
                            px(theme::icon_medium()),
                            theme::TEXT_MUTED,
                        ));
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
                        .child(restore)
                        .into_any_element()
                } else {
                    let store = self.store.clone();
                    let notes_folder = folder.clone();
                    let notes_name = repo.name.clone();
                    let notes_button = div()
                        .id(("open-notes", row_ix))
                        .cursor_pointer()
                        .flex_shrink_0()
                        .rounded_md()
                        .p_1p5()
                        .flex()
                        .items_center()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            store.update(cx, |s, cx| {
                                s.ensure_repo_selected(&notes_folder, cx);
                                s.request_notes_scope(notes_name.clone(), cx);
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
                        .child(crate::ui::with_tooltip(
                            notes_button,
                            show_tooltips,
                            "Open notes for this repository",
                        ))
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
                    let warp_running = store
                        .warp_running
                        .get(&folder)
                        .filter(|status| !status.running.is_empty())
                        .map(|status| status.running.join(", "));
                    shortcuts_cell(repo, &settings, row_ix, &self.store, warp_running)
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
                let cell = div()
                    .flex()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(
                        repo.git_last_commit_at
                            .map(devtools_core::rel_time::format)
                            .unwrap_or_else(|| "—".into()),
                    )
                    .into_any_element();
                if archived {
                    faded(cell)
                } else {
                    cell
                }
            }
        };

        // the kit's cell wrapper is a block div and percentage heights don't
        // resolve through it, so cell content would hug the row top; every
        // cell centers inside an explicit flex column instead. The height
        // must leave the kit's own 4+4 cell padding free — at the full row
        // height the column overflows 8px and content rides ~4px low.
        // Archived rows also register the hover group the restore icon
        // reveals under.
        let mut wrap = div()
            .debug_selector(move || format!("repos-td-wrap-{row_ix}"))
            .h(px(theme::TABLE_ROW_H - 8.))
            .flex()
            .flex_col()
            .justify_center();
        if archived {
            wrap = wrap.group("archived-row");
        }
        wrap.child(cell).into_any_element()
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
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            let folder = folder.clone();
            store.update(cx, |s, cx| s.open_repo_tab(&folder, tab, cx));
        })
        .child(cell)
        .into_any_element()
}

fn branch_pill(branch: Option<&str>) -> gpui::AnyElement {    let Some(branch) = branch.filter(|b| !b.is_empty()) else {
        return div().text_color(theme::TEXT_FAINT).child("—").into_any_element();
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
    div()
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
        .child(icon("icons/icon-source-branch.svg", px(theme::icon_small() - 2.), color))
        .child(branch.to_string())
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
                .child(icon("icons/icon-git-pull-request.svg", px(theme::icon_small() - 3.), theme::PURPLE_BRIGHT))
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
                .child(icon("icons/icon-issue.svg", px(theme::icon_small() - 3.), theme::SUCCESS_BRIGHT))
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
    if snapshot.pipelines.is_empty() && snapshot.pulls.is_empty() && snapshot.work_items.is_empty() {
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
/// [zcode][solution][vscode][folder][terminal]. Purely visibility switches from
/// settings — a configured-but-missing executable reports at launch time.
fn shortcuts_cell(
    repo: &RepoEntry,
    settings: &AppSettings,
    row_ix: usize,
    store: &Entity<Store>,
    warp_running: Option<String>,
) -> gpui::AnyElement {
    use devtools_core::terminal;
    let folder = repo.folder_path.to_string_lossy().to_string();
    let show_tooltips = settings.general.show_tooltips;
    let mut row = div().flex().flex_row().items_center().gap_1p5();

    // a live warp-agent turn in this repo's workspace: leading sparkle
    if let Some(titles) = warp_running {
        let element = div()
            .id(SharedString::from(format!("warp-running-{row_ix}")))
            .flex()
            .items_center()
            .child(icon("icons/icon-agent-sparkle.svg", px(theme::icon_small()), theme::SUCCESS_BRIGHT));
        row = row.child(crate::ui::with_tooltip(
            element,
            show_tooltips,
            SharedString::from(format!("warp-agent is running: {titles}")),
        ));
    }

    // the agent door: opens the repo as a warp-agent workspace (added when
    // missing, launched when closed)
    if settings.warp_agent.enable_warp_agent {
        let store = store.clone();
        let folder_for_open = folder.clone();
        let element = div()
            .id(SharedString::from(format!("open-warp-{row_ix}")))
            .debug_selector(move || format!("repos-warp-cell-{row_ix}"))
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

    // OpenCode quick-launch: one opencode instance in the repo with the
    // default model; without a configured default there is nothing to launch
    if settings.opencode.enable_opencode {
        let model = settings.opencode.default_model.trim().to_string();
        let has_model = !model.is_empty();
        let launch: Box<dyn Fn() -> Result<(), String>> = if has_model {
            Box::new({
                let folder = folder.clone();
                let term = settings.repos.terminal_executable.clone();
                let exe = if settings.repos.opencode_executable.trim().is_empty() {
                    "opencode".to_string()
                } else {
                    settings.repos.opencode_executable.trim().to_string()
                };
                move || {
                    terminal::launch_argv_in_terminal(
                        &folder,
                        &[exe.clone(), "--model".into(), model.clone()],
                        &term,
                    )
                }
            })
        } else {
            Box::new(|| Err("No default OpenCode model — set it in Settings".into()))
        };
        row = row.child(button(
            format!("open-opencode-{row_ix}"),
            "icons/icon-opencode.svg",
            theme::TEXT_MUTED,
            "Launch OpenCode with the default model",
            launch,
        ));
        // .NET separates the opencode group from the launch buttons
        row = row.child(
            div()
                .mx_1()
                .self_center()
                .h(px(20.))
                .w(px(1.))
                .bg(theme::HAIRLINE),
        );
    }

    if settings.repos.enable_zcode {
        row = row.child(button(
            format!("open-zcode-{row_ix}"),
            "icons/icon-zcode.svg",
            theme::TEXT_MUTED,
            "Open in ZCode",
            Box::new({
                let folder = folder.clone();
                let term = settings.repos.terminal_executable.clone();
                let zcode = settings.repos.zcode_executable.clone();
                move || terminal::launch_in_terminal(&folder, &zcode, &term)
            }),
        ));
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
        .cursor_pointer()
        .rounded_md()
        .h(px(30.))
        .px_2p5()
        .flex()
        .items_center()
        .gap_1p5()
        .text_size(px(theme::font_small()))
        .when(busy, |d| d.text_color(theme::ACCENT))
        .when(!busy, |d| d.text_color(theme::TEXT_DIM))
        .bg(theme::CHIP)
        .hover(|s| s.opacity(0.9))
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
        .child(if busy { "summarizing…" } else { "Summary" })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_with(launchers: &[&str]) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.repos.enable_terminal = launchers.contains(&"terminal");
        settings.repos.enable_vscode = launchers.contains(&"vscode");
        settings.repos.enable_zcode = launchers.contains(&"zcode");
        settings.repos.enable_visual_studio = launchers.contains(&"sln");
        settings.opencode.enable_opencode = launchers.contains(&"opencode");
        settings.warp_agent.enable_warp_agent = launchers.contains(&"warp");
        settings
    }

    #[test]
    fn shortcuts_column_hugs_the_enabled_launchers() {
        // folder only: one 36px button, no gaps, no divider, slack 8
        assert_eq!(shortcuts_column_width(&settings_with(&[])), px(44.));
        // folder + terminal: two buttons + one gap
        assert_eq!(shortcuts_column_width(&settings_with(&["terminal"])), px(86.));
        // full linux set: warp door, opencode button + divider, zcode, vscode,
        // folder, terminal = 6 buttons + divider = 7 children -> 6 gaps +
        // divider (1px + mx_1 = 9); the solution slot only joins on Windows
        let full = shortcuts_column_width(&settings_with(&[
            "terminal", "vscode", "zcode", "opencode", "warp",
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
            cx.read_entity(&store, |s, _| s.selected_repo.clone()).as_deref(),
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
