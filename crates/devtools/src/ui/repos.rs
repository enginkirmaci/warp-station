//! The Repos page: toolbar (search, sort, tag chips, refresh, totals) and the
//! repos table. The table delegate reads the store's projection at render
//! time, so filter/sort/status changes land with the next frame — no mirrored
//! row list to keep in sync.

use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::dialogs::{AddRepoDialog, CloneDialog, Dialog};
use crate::ui::icon;
use devtools_core::repo_store::RepoEntry;
use devtools_core::settings::{AppSettings, RepoSortMode};
use gpui::prelude::*;
use gpui::{div, px, App, Context, Entity, FontWeight, Pixels, SharedString, Window};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Sizable as _, Size};
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableEvent, TableState};
use std::cell::Cell;
use std::rc::Rc;

pub struct ReposPage {
    pub store: Entity<Store>,
    table: Entity<TableState<ReposDelegate>>,
    search: Entity<InputState>,
    sort_menu_open: bool,
    pub dialog: Option<Dialog>,
    /// The table's available width, written by render each frame; the
    /// delegate distributes it across the columns (the kit's table has no
    /// flex columns — its widths only re-prepare on `TableState::refresh`).
    available_width: Rc<Cell<Pixels>>,
    /// Fingerprint of everything the column widths depend on (available
    /// width, shortcut toggles, widest branch); a change triggers one refresh.
    layout_fingerprint: Cell<u64>,
}

impl ReposPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store_for_search = store.clone();
        let store_for_table = store.clone();
        let store_for_observe = store.clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search repositories"));
        let available_width = Rc::new(Cell::new(px(1280.)));
        let table = cx.new(|cx| {
            TableState::new(
                ReposDelegate {
                    store: store.clone(),
                    available_width: available_width.clone(),
                },
                window,
                cx,
            )
            .row_selectable(true)
        });

        let search_for_store = search.clone();
        cx.subscribe(&search, move |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = search_for_store.read(cx).value().to_string();
                store_for_search.update(cx, |s, cx| s.set_search(text, cx));
            }
        })
        .detach();

        let table_for_store = table.clone();
        cx.subscribe(&table, move |_, _, event: &TableEvent, cx| {
            if let TableEvent::SelectRow(row_ix) = event {
                let folder = table_for_store.read(cx).delegate().folder_at(*row_ix, cx);
                tracing::info!("row {row_ix} selected -> {folder:?}");
                store_for_table.update(cx, |s, cx| s.select_repo(folder, cx));
            }
        })
        .detach();

        cx.observe(&store_for_observe, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            table,
            search,
            sort_menu_open: false,
            dialog: None,
            available_width,
            layout_fingerprint: Cell::new(0),
        }
    }
}

impl ReposPage {
    /// Packs everything the column widths depend on into one u64: available
    /// width, the shortcut-toggle flags, and the widest branch in the current
    /// projection.
    fn layout_fingerprint_value(&self, available: Pixels, cx: &Context<Self>) -> u64 {
        let settings = self.store.read(cx).settings();
        let mut fp = f32::from(available).to_bits() as u64;
        let mut flag = |bit: u32, on: bool| {
            if on {
                fp ^= 1 << bit;
            }
        };
        flag(1, settings.opencode.enable_opencode);
        flag(2, settings.repos.enable_zcode);
        flag(3, cfg!(windows) && settings.repos.enable_visual_studio);
        flag(4, settings.repos.enable_vscode);
        flag(5, settings.repos.enable_terminal);
        flag(6, settings.repos.enable_git_hub);
        flag(7, settings.repos.enable_azure_dev_ops);
        fp ^= (widest_branch_chars(&self.store, cx) as u64) << 8;
        fp
    }

    /// Sends the Add-Repositories dialog's deep scan.
    pub fn kick_deep_scan(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::AddRepo(dialog)) = &mut self.dialog else { return };
        let root = dialog.root.read(cx).value().to_string();
        if root.trim().is_empty() {
            return;
        }
        dialog.scanning = true;
        dialog.found.clear();
        let settings = self.store.read(cx).settings();
        let _ = self.store.read(cx).net.send_blocking(crate::net::NetCommand::DeepScan {
            root: root.trim().to_string(),
            depth: settings.repos.max_scan_depth,
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
            let name = dialog.derived_name(cx);
            let destination = dialog.destination.clone();
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
        let name = dialog.derived_name(cx);
        let destination = dialog.destination.clone();
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
        // fingerprint change (window resize, shortcut toggles, widest branch).
        let available = window.viewport_size().width
            - px(24.) // the px_3 gutters around the table
            - px(14.); // vertical scrollbar + slack
        let fingerprint = self.layout_fingerprint_value(available, cx);
        if fingerprint != self.layout_fingerprint.get() {
            self.layout_fingerprint.set(fingerprint);
            self.available_width.set(available);
            self.table.update(cx, |state, cx| state.refresh(cx));
        }
        let mut page = div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .relative()
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px_3()
                    .pb_2()
                    .child(DataTable::new(&self.table).with_size(Size::Size(px(36.))).stripe(false).bordered(false)),
            );
        if self.sort_menu_open {
            page = page.child(self.render_sort_overlay(cx));
        }
        if let Some(dialog) = &mut self.dialog {
            page = page.child(dialog.render(window, cx));
        }
        page
    }
}

impl ReposPage {
    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (tags, active_tag, scanning, summary_busy, notice) = {
            let store = self.store.read(cx);
            let notice = store
                .notice
                .as_ref()
                .filter(|(_, at)| at.elapsed() < std::time::Duration::from_secs(4))
                .map(|(text, _)| text.clone());
            (store.all_tags(), store.tag_filters.clone(), store.scanning, store.summary_busy, notice)
        };

        // header count chips: repos always; changes when non-zero; PRs and
        // issues together when the GitHub column is on and either is open
        let store = self.store.read(cx);
        let total_repos = store.repos.len();
        let total_changes: u32 = store.repos.iter().map(|r| r.git_modified_count).sum();
        let total_prs: u32 = store.repos.iter().map(|r| r.github_pr_count).sum();
        let total_issues: u32 = store.repos.iter().map(|r| r.github_issue_count).sum();
        let github_on = store.settings().repos.enable_git_hub;

        let count_chip = |count: u32, color: gpui::Rgba, icon_path: Option<&'static str>| {
            div()
                .rounded_md()
                .px_1p5()
                .py_0p5()
                .text_size(px(theme::font_small()))
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .bg(theme::tint(color))
                .text_color(color)
                .children(icon_path.map(|path| icon(path, px(theme::icon_small() - 2.), color)))
                .child(format!("{count}"))
        };

        // filter checkboxes: OR semantics across every checked tag
        let mut filters = div().flex().flex_row().items_center().gap_3();
        for tag in &tags {
            let active = active_tag.iter().any(|t| t.eq_ignore_ascii_case(tag));
            filters = filters.child(
                div()
                    .id(gpui::ElementId::NamedInteger("filter".into(), index_of_tag(&tags, tag) as u64))
                    .cursor_pointer()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .hover(|s| s.opacity(0.85))
                    .on_click({
                        let tag = tag.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.store.update(cx, |s, cx| s.toggle_tag_filter(&tag, cx));
                        })
                    })
                    .child(
                        div()
                            .rounded_xs()
                            .size(px(13.))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_color(theme::TEXT_FAINT)
                            .when(active, |d| d.bg(theme::ACCENT).border_color(theme::ACCENT))
                            .when(active, |d| {
                                d.child(icon("icons/icon-check.svg", px(9.), theme::ON_ACCENT))
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .when(active, |d| d.text_color(theme::TEXT).font_weight(FontWeight::SEMIBOLD))
                            .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                            .child(tag.clone()),
                    ),
            );
        }

        let store_handle_refresh = self.store.clone();
        let store_handle_clear = self.store.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .relative()
            .child(icon("icons/icon-logo-mark.svg", px(theme::icon_medium()), theme::TEXT_MUTED))
            .child(
                div()
                    .text_size(px(theme::font_title() + 2.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme::TEXT)
                    .child("Repositories"),
            )
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(format!("{}", total_repos)),
            )
            .when(total_changes > 0, |d| d.child(count_chip(total_changes, theme::AMBER_BRIGHT, None)))
            .when(github_on && (total_prs > 0 || total_issues > 0), |d| {
                d.child(count_chip(total_prs, theme::PURPLE_BRIGHT, Some("icons/icon-git-pull-request.svg")))
                    .child(count_chip(total_issues, theme::SUCCESS_BRIGHT, Some("icons/icon-issue.svg")))
            })
            .child(div().w(px(8.)))
            .child(filters)
            .child(div().flex_1())
            .when_some(notice, |row, notice| {
                row.child(
                    div()
                        .max_w(px(320.))
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_MUTED)
                        .overflow_hidden()
                        .child(notice),
                )
            })
            .child(summary_button(&self.store, summary_busy))
            .child(
                // sort select: bordered box with up/down chevrons
                div()
                    .id("sort-button")
                    .debug_selector(|| "dt-sort-chip".into())
                    .cursor_pointer()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .px_2p5()
                    .py_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .hover(|s| s.border_color(theme::TEXT_FAINT))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sort_menu_open = !this.sort_menu_open;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::TEXT_DIM)
                            .child({
                let store = self.store.read(cx);
                sort_label(store.sort_mode).to_string()
            }),
                    )
                    .child(div().text_size(px(theme::font_small() - 2.)).text_color(theme::TEXT_FAINT).child("⇅")),
            )
            .child(
                div()
                    .id("clear-button")
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let search = self.search.clone();
                        move |_, window, cx| {
                            search.update(cx, |state, cx| state.set_value("", window, cx));
                            store_handle_clear.update(cx, |s, cx| s.clear_filters(cx));
                        }
                    })
                    .child("Clear"),
            )
            .child(
                div()
                    .id("refresh-button")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(move |_, _, cx| {
                        store_handle_refresh.update(cx, |s, cx| s.refresh(cx));
                    })
                    .child(icon(
                        "icons/icon-refresh.svg",
                        px(theme::icon_small()),
                        if scanning { theme::ACCENT } else { theme::TEXT_MUTED },
                    )),
            )
            .child(
                div()
                    .id("add-repo-button")
                    .cursor_pointer()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::ACCENT)
                    .px_3()
                    .py_1p5()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .hover(|s| s.bg(theme::tint(theme::ACCENT)))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dialog = Some(Dialog::AddRepo(AddRepoDialog::new(window, cx)));
                        cx.notify();
                    }))
                    .child(div().text_color(theme::ACCENT).text_size(px(theme::font_small())).child("+"))
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::ACCENT)
                            .child("Add Repository"),
                    ),
            )
            .child(
                div()
                    .id("clone-button")
                    .cursor_pointer()
                    .rounded_md()
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .px_3()
                    .py_1p5()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let store = this.store.clone();
                        this.dialog = Some(Dialog::Clone(CloneDialog::new(&store, window, cx)));
                        cx.notify();
                    }))
                    .child(icon("icons/icon-cloud-download.svg", px(theme::icon_small()), theme::TEXT_DIM))
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::TEXT_DIM)
                            .child("Clone from URL"),
                    ),
            )
            .into_any_element()
    }

    /// The sort dropdown as a root-level overlay: a full-page click-catcher
    /// plus the menu under the sort chip. It cannot live inside the toolbar
    /// row — later siblings (the table) paint over toolbar overflow, burying
    /// the menu, and a row-sized catcher never sees clicks outside it.
    fn render_sort_overlay(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .id("sort-menu-catcher")
                    .debug_selector(|| "dt-sort-scrim".into())
                    .absolute()
                    .inset_0()
                    .occlude()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sort_menu_open = false;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .absolute()
                    .debug_selector(|| "dt-sort-menu".into())
                    .top(px(48.))
                    // right edge of the menu = right edge of the sort chip:
                    // 12px toolbar padding + Clone/Add/refresh/Clear buttons
                    // and gaps to the chip's right
                    .right(px(370.))
                    .min_w(px(150.))
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .bg(theme::POPUP)
                    .p_1()
                    .shadow_lg()
                    .child(self.render_sort_menu(cx)),
            )
            .into_any_element()
    }

    fn render_sort_menu(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let current = self.store.read(cx).sort_mode;
        let modes = [
            (RepoSortMode::Name, "Name"),
            (RepoSortMode::LastActivity, "Last activity"),
            (RepoSortMode::Changes, "Changes"),
            (RepoSortMode::PullRequests, "Pull requests"),
            (RepoSortMode::Issues, "Issues"),
        ];
        let mut list = div().flex().flex_col();
        for (label_index, (mode, label)) in modes.iter().enumerate() {
            let selected = *mode == current;
            let mode = *mode;
            list = list.child(
                div()
                    .id(gpui::ElementId::NamedInteger("sort-item".into(), label_index as u64))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::font_small()))
                    .when(selected, |d| {
                        d.text_color(theme::ACCENT).font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!selected, |d| d.text_color(theme::TEXT_DIM))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.store.update(cx, |s, cx| s.set_sort(mode, cx));
                        this.sort_menu_open = false;
                        cx.notify();
                    }))
                    .child(*label),
            );
        }
        list.into_any_element()
    }
}

fn index_of_tag(tags: &[String], tag: &str) -> usize {
    tags.iter().position(|t| t == tag).unwrap_or(0)
}

/// The shortcuts column hugs its button row: 28px buttons, 6px gaps, and the
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
    let divider = settings.opencode.enable_opencode;
    if divider {
        buttons += 1; // the opencode button itself
    }
    let children = buttons + divider as usize;
    let gaps = children.saturating_sub(1) as f32 * 6.0;
    px(buttons as f32 * 28.0 + if divider { 9.0 } else { 0.0 } + gaps + 8.0)
}

/// The widest branch name in the current projection, in characters — the
/// branch column hugs it so long branch names get room and short ones don't
/// waste 240px.
fn widest_branch_chars(store: &Entity<Store>, cx: &App) -> usize {
    let store = store.read(cx);
    store
        .projected
        .iter()
        .filter_map(|&ix| store.repos.get(ix))
        .filter_map(|repo| repo.git_branch_name.as_deref())
        .filter(|branch| !branch.is_empty())
        .map(|branch| branch.chars().count())
        .max()
        .unwrap_or(6)
}

/// Branch column width from the widest branch pill: git glyph, pill padding,
/// ~7.2px per character in the code font, clamped to sane bounds.
fn branch_column_width(store: &Entity<Store>, cx: &App) -> Pixels {
    let chars = widest_branch_chars(store, cx) as f32;
    px(52. + chars * 7.2).clamp(px(110.), px(240.))
}

fn sort_label(mode: RepoSortMode) -> &'static str {
    match mode {
        RepoSortMode::Name => "Name",
        RepoSortMode::LastActivity => "Last activity",
        RepoSortMode::Changes => "Changes",
        RepoSortMode::PullRequests => "Pull requests",
        RepoSortMode::Issues => "Issues",
    }
}

// ---- the table --------------------------------------------------------------

/// Which column sits at an index — the GitHub/Azure columns exist only when
/// their settings enable them, so everything resolves through this.
#[derive(Clone, Copy, PartialEq)]
enum ColKind {
    Star,
    Name,
    Shortcuts,
    Branch,
    Changes,
    Github,
    Azure,
    Activity,
}

fn column_kind(col_ix: usize, settings: &AppSettings) -> ColKind {
    match col_ix {
        0 => ColKind::Star,
        1 => ColKind::Name,
        2 => ColKind::Shortcuts,
        3 => ColKind::Branch,
        4 => ColKind::Changes,
        _ => {
            let mut ix = 5;
            if settings.repos.enable_git_hub {
                if col_ix == ix {
                    return ColKind::Github;
                }
                ix += 1;
            }
            if settings.repos.enable_azure_dev_ops && col_ix == ix {
                return ColKind::Azure;
            }
            ColKind::Activity
        }
    }
}

pub struct ReposDelegate {
    pub store: Entity<Store>,
    /// The table's available width, kept current by the page's render; see
    /// `ReposPage::available_width`.
    pub available_width: Rc<Cell<Pixels>>,
}

impl ReposDelegate {
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
}

impl TableDelegate for ReposDelegate {
    /// The list has no column headers — the toolbar carries sort/filter —
    /// so the header row collapses to zero height (refine_style applies these
    /// after the kit's own h_flex/background styling).
    fn render_header(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("repos-header")
            .h_0()
            .overflow_hidden()
            .text_color(gpui::transparent_black())
    }

    fn columns_count(&self, cx: &App) -> usize {
        let settings = self.store.read(cx).settings();
        6 + settings.repos.enable_git_hub as usize + settings.repos.enable_azure_dev_ops as usize
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.store.read(cx).projected.len()
    }

    fn column(&self, col_ix: usize, cx: &App) -> Column {
        let settings = self.store.read(cx).settings();
        // fixed-width columns scale with the font factor so scaled text keeps
        // fitting; the name column absorbs whatever is left over
        let scale = theme::font_small() / 12.0;
        let mut fixed = px(36.) // star
            + shortcuts_column_width(&settings)
            + px(150.) * scale // changes
            + px(84.) * scale; // activity
        if settings.repos.enable_git_hub {
            fixed += px(96.) * scale;
        }
        if settings.repos.enable_azure_dev_ops {
            fixed += px(120.) * scale;
        }
        let branch = branch_column_width(&self.store, cx);
        let name = (self.available_width.get() - fixed - branch).max(px(140.));
        match column_kind(col_ix, &settings) {
            ColKind::Star => {
                Column::new("star", "").width(px(36.)).resizable(false).text_center()
            }
            ColKind::Name => Column::new("name", "Repository").width(name).sortable().min_width(px(140.)),
            ColKind::Shortcuts => {
                // hugs its button row: the width follows the enabled launchers
                Column::new("shortcuts", "").width(shortcuts_column_width(&settings)).resizable(false)
            }
            ColKind::Branch => Column::new("branch", "Branch").width(branch).resizable(false),
            ColKind::Changes => Column::new("changes", "Changes").width(px(150.) * scale).resizable(false).sortable(),
            ColKind::Github => Column::new("github", "GitHub").width(px(96.) * scale).sortable(),
            ColKind::Azure => {
                Column::new("azure", "Azure DevOps").width(px(120.) * scale).text_center().resizable(false)
            }
            ColKind::Activity => {
                Column::new("activity", "Last activity").width(px(84.) * scale).resizable(false).sortable()
            }
        }
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        _sort: gpui_kit::component::table::ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let settings = self.store.read(cx).settings();
        let mode = match column_kind(col_ix, &settings) {
            ColKind::Name => Some(RepoSortMode::Name),
            ColKind::Changes => Some(RepoSortMode::Changes),
            ColKind::Github => Some(RepoSortMode::PullRequests),
            ColKind::Activity => Some(RepoSortMode::LastActivity),
            _ => None,
        };
        if let Some(mode) = mode {
            let store = self.store.clone();
            store.update(cx, |s, cx| s.set_sort(mode, cx));
        }
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let Some(repo) = self.repo_at(row_ix, cx) else {
            return div().into_any_element();
        };
        let folder = repo.folder_path.to_string_lossy().to_string();
        let settings = self.store.read(cx).settings();

        match column_kind(col_ix, &settings) {
            ColKind::Star => {
                let favorite = repo.is_favorite();
                div()
                    .id(("star", row_ix))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(26.))
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
                        px(theme::icon_small()),
                        if favorite { theme::GOLD_BRIGHT } else { theme::STAR_MUTED },
                    ))
                    .into_any_element()
            }
            ColKind::Name => {
                let store = self.store.clone();
                let folder = folder.clone();
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        // notes: selects the repo and flags the floating notes
                        // window open — always opens/switches, never toggles
                        div()
                            .id(("open-notes", row_ix))
                            .cursor_pointer()
                            .rounded_md()
                            .p_1()
                            .flex()
                            .items_center()
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                store.update(cx, |s, cx| {
                                    s.ensure_repo_selected(&folder, cx);
                                    s.notes_requested = true;
                                });
                            })
                            .child(icon(
                                "icons/icon-note.svg",
                                px(theme::icon_small()),
                                theme::TEXT_MUTED,
                            )),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(theme::font_body() + 1.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(repo.name.clone()),
                    )
                    .into_any_element()
            }
            ColKind::Shortcuts => shortcuts_cell(&repo, &settings, row_ix, &self.store),
            // branch pill and changes cell open the bar's Changes tab; the
            // GitHub/Azure cells route to their own tabs
            ColKind::Branch => tab_cell(
                ("open-changes", row_ix as u64),
                &folder,
                BottomBarTab::Changes,
                &self.store,
                branch_pill(repo.git_branch_name.as_deref()),
            ),
            ColKind::Changes => tab_cell(
                ("open-changes", row_ix as u64),
                &folder,
                BottomBarTab::Changes,
                &self.store,
                changes_cell(&repo),
            ),
            ColKind::Github => {
                let tab = if repo.github_pr_count > 0 {
                    BottomBarTab::PullRequests
                } else {
                    BottomBarTab::Issues
                };
                let has_content = repo.github_pr_count > 0 || repo.github_issue_count > 0;
                if has_content {
                    tab_cell(("open-gh", row_ix as u64), &folder, tab, &self.store, github_cell(&repo))
                } else {
                    github_cell(&repo)
                }
            }
            ColKind::Azure => tab_cell(
                ("open-azure", row_ix as u64),
                &folder,
                BottomBarTab::Azure,
                &self.store,
                azure_cell(&repo, &self.store, cx),
            ),
            ColKind::Activity => div()
                .flex()
                .justify_end()
                .pr_1()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_MUTED)
                .child(
                    repo.git_last_commit_at
                        .map(devtools_core::rel_time::format)
                        .unwrap_or_else(|| "—".into()),
                )
                .into_any_element(),
        }
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
            (store.scanning, !store.search.trim().is_empty() || !store.tag_filters.is_empty(), has_folders)
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
        .text_size(px(theme::font_small() - 1.))
        .font_family(gpui::SharedString::from(theme::code_font()))
        .text_color(color)
        .child(icon("icons/icon-source-branch.svg", px(theme::icon_small() - 3.), color))
        .child(branch.to_string())
        .into_any_element()
}

/// The changes cell: `[amber modified] Changes (push, pull)` with the .NET
/// arrow glyphs, plus a gray check when the branch is synced — rendered in
/// every state, never "clean".
fn changes_cell(repo: &RepoEntry) -> gpui::AnyElement {
    let synced = repo.git_to_push_count == 0 && repo.git_to_pull_count == 0;
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .text_size(px(theme::font_small()));
    row = row.child(
        div()
            .rounded_md()
            .bg(theme::tint(theme::AMBER_BRIGHT))
            .px_1p5()
            .py_0p5()
            .text_size(px(theme::font_small() - 1.))
            .text_color(theme::AMBER_BRIGHT)
            .child(format!("{}", repo.git_modified_count)),
    );
    row = row
        .child(div().text_color(theme::TEXT_DIM).child("Changes"))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_0p5()
                .text_color(theme::TEXT_MUTED)
                .child("(")
                .child(icon("icons/icon-arrow-up.svg", px(theme::icon_small() - 4.), theme::SUCCESS_BRIGHT))
                .child(div().text_color(theme::SUCCESS_BRIGHT).child(format!("{}", repo.git_to_push_count)))
                .child(div().child(","))
                .child(icon("icons/icon-arrow-down.svg", px(theme::icon_small() - 4.), theme::BLUE_BRIGHT))
                .child(div().text_color(theme::BLUE_BRIGHT).child(format!("{}", repo.git_to_pull_count)))
                .child(")"),
        );
    let _ = synced;
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
        row = row.child(icon("icons/icon-check.svg", px(theme::icon_small() - 3.), theme::TEXT_MUTED));
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
) -> gpui::AnyElement {
    use devtools_core::terminal;
    let folder = repo.folder_path.to_string_lossy().to_string();
    let mut row = div().flex().flex_row().items_center().gap_1p5();

    let button = |id: String, icon_path: &'static str, tint_color: gpui::Rgba, launch: Box<dyn Fn() -> Result<(), String>>| {
        let store = store.clone();
        div()
            .id(SharedString::from(id))
            .cursor_pointer()
            .rounded_md()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::CARD)
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .hover(|s| s.border_color(theme::TEXT_FAINT))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                if let Err(err) = launch() {
                    store.update(cx, |s, cx| {
                        s.notice = Some((err, std::time::Instant::now()));
                        cx.notify();
                    });
                }
            })
            .child(icon(icon_path, px(theme::icon_small()), tint_color))
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
            launch,
        ));
        // .NET separates the opencode group from the launch buttons
        row = row.child(
            div()
                .mx_1()
                .self_center()
                .h(px(16.))
                .w(px(1.))
                .bg(theme::HAIRLINE),
        );
    }

    if settings.repos.enable_zcode {
        row = row.child(button(
            format!("open-zcode-{row_ix}"),
            "icons/icon-zcode.svg",
            theme::TEXT_MUTED,
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
                    Box::new(move || terminal::open_solution(&solution, &ide)),
                ));
            }
            None => row = row.child(div().size(px(28.))),
        }
    }
    if settings.repos.enable_vscode {
        row = row.child(button(
            format!("open-vscode-{row_ix}"),
            "icons/icon-vscode.svg",
            theme::BLUE_BRIGHT,
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
            Box::new({
                let folder = folder.clone();
                let term = settings.repos.terminal_executable.clone();
                move || terminal::open_terminal(&folder, &term)
            }),
        ));
    }
    row.into_any_element()
}

fn summary_button(store: &Entity<Store>, busy: bool) -> gpui::AnyElement {
    let handle = store.clone();
    div()
        .id("summary-button")
        .cursor_pointer()
        .rounded_md()
        .px_2()
        .py_1()
        .flex()
        .items_center()
        .gap_1()
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
        settings
    }

    #[test]
    fn shortcuts_column_hugs_the_enabled_launchers() {
        // folder only: one 28px button, no gaps, no divider, slack 8
        assert_eq!(shortcuts_column_width(&settings_with(&[])), px(36.));
        // folder + terminal: two buttons + one gap
        assert_eq!(shortcuts_column_width(&settings_with(&["terminal"])), px(70.));
        // full linux set: opencode button + divider, zcode, vscode, folder,
        // terminal = 5 buttons + divider = 6 children -> 5 gaps + divider
        // (1px + mx_1 = 9); the solution slot only joins on Windows
        let full =
            shortcuts_column_width(&settings_with(&["terminal", "vscode", "zcode", "opencode"]));
        assert_eq!(full, px(5. * 28. + 5. * 6. + 9. + 8.));
    }
}
