//! The Repos page: toolbar (search, sort, tag chips, refresh, totals) and the
//! repos table. The table delegate reads the store's projection at render
//! time, so filter/sort/status changes land with the next frame — no mirrored
//! row list to keep in sync.

use crate::store::Store;
use crate::theme;
use crate::ui::dialogs::{AddRepoDialog, CloneDialog, Dialog};
use crate::ui::icon;
use devtools_core::repo_store::RepoEntry;
use devtools_core::settings::{AppSettings, RepoSortMode};
use gpui::prelude::*;
use gpui::{div, px, App, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableEvent, TableState};

pub struct ReposPage {
    pub store: Entity<Store>,
    table: Entity<TableState<ReposDelegate>>,
    search: Entity<InputState>,
    sort_menu_open: bool,
    pub dialog: Option<Dialog>,
}

impl ReposPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store_for_search = store.clone();
        let store_for_table = store.clone();
        let store_for_observe = store.clone();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search repositories"));
        let table = cx.new(|cx| {
            TableState::new(ReposDelegate { store: store.clone() }, window, cx).row_selectable(true)
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
        Self { store, table, search, sort_menu_open: false, dialog: None }
    }
}

impl ReposPage {
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

    /// Sends the Clone dialog's clone.
    pub fn commit_clone(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Clone(dialog)) = &mut self.dialog else { return };
        let url = dialog.url.read(cx).value().to_string();
        if url.trim().is_empty() {
            return;
        }
        dialog.busy = true;
        dialog.error = None;
        let name = dialog.derived_name(cx);
        let destination = dialog.destination.clone();
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
        if let Some(found) = drained_deep_scan {
            if let Some(Dialog::AddRepo(dialog)) = &mut self.dialog {
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
        let mut page = div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .relative()
            .child(self.render_toolbar(cx))
            .child(self.render_totals(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .px_3()
                    .pb_2()
                    .child(DataTable::new(&self.table).stripe(false).bordered(false)),
            );
        if let Some(dialog) = &mut self.dialog {
            page = page.child(dialog.render(window, cx));
        }
        page
    }
}

impl ReposPage {
    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (tags, active_tag, scanning) = {
            let store = self.store.read(cx);
            (store.all_tags(), store.tag_filter.clone(), store.scanning)
        };

        let mut chips = div().flex().flex_row().gap_1();
        for (index, tag) in tags.iter().enumerate() {
            let active = active_tag.as_deref() == Some(tag.as_str());
            chips = chips.child(
                div()
                    .id(gpui::ElementId::NamedInteger("tag-chip".into(), index as u64))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_0p5()
                    .text_size(px(theme::FONT_SMALL))
                    .when(active, |d| {
                        d.bg(theme::ACCENT)
                            .text_color(theme::ON_ACCENT)
                            .font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!active, |d| d.bg(theme::CHIP).text_color(theme::TEXT_MUTED))
                    .hover(|s| s.opacity(0.9))
                    .on_click({
                        let tag = tag.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.store.update(cx, |s, cx| s.toggle_tag_filter(&tag, cx));
                        })
                    })
                    .child(tag.clone()),
            );
        }

        let store_handle = self.store.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .relative()
            .child(div().w(px(220.)).child(Input::new(&self.search).small()))
            .child(chips)
            .child(div().flex_1())
            .child(
                div()
                    .id("add-repo-button")
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::FONT_SMALL))
                    .text_color(theme::TEXT_DIM)
                    .bg(theme::CHIP)
                    .hover(|s| s.opacity(0.9))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.dialog = Some(Dialog::AddRepo(AddRepoDialog::new(window, cx)));
                        cx.notify();
                    }))
                    .child("+ Add"),
            )
            .child(
                div()
                    .id("clone-button")
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::FONT_SMALL))
                    .text_color(theme::TEXT_DIM)
                    .bg(theme::CHIP)
                    .hover(|s| s.opacity(0.9))
                    .on_click({
                        let store = self.store.clone();
                        cx.listener(move |this, _, window, cx| {
                            this.dialog = Some(Dialog::Clone(CloneDialog::new(&store, window, cx)));
                            cx.notify();
                        })
                    })
                    .child("Clone"),
            )
            .child(
                div()
                    .id("sort-button")
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::FONT_SMALL))
                    .text_color(theme::TEXT_DIM)
                    .bg(theme::CHIP)
                    .hover(|s| s.opacity(0.9))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sort_menu_open = !this.sort_menu_open;
                        cx.notify();
                    }))
                    .child(format!("Sort: {}", sort_label(self.sort_mode(cx)))),
            )
            .child(
                div()
                    .id("refresh-button")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(move |_, _, cx| {
                        store_handle.update(cx, |s, cx| s.refresh(cx));
                    })
                    .child(icon(
                        "icons/icon-arrow-down.svg",
                        px(theme::ICON_SMALL),
                        if scanning { theme::ACCENT } else { theme::TEXT_MUTED },
                    )),
            )
            .when(self.sort_menu_open, |toolbar| {
                toolbar.child(
                    div()
                        .id("sort-menu-catcher")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sort_menu_open = false;
                            cx.notify();
                        }))
                        .child(
                            div()
                                .absolute()
                                .right(px(48.))
                                .top(px(40.))
                                .min_w(px(150.))
                                .rounded_lg()
                                .border_1()
                                .border_color(theme::HAIRLINE)
                                .bg(theme::POPUP)
                                .p_1()
                                .shadow_lg()
                                .child(self.render_sort_menu(cx)),
                        ),
                )
            })
            .into_any_element()
    }

    fn sort_mode(&self, cx: &Context<Self>) -> RepoSortMode {
        self.store.read(cx).sort_mode
    }

    fn render_sort_menu(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let current = self.sort_mode(cx);
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
            list = list.child(
                div()
                    .id(gpui::ElementId::NamedInteger("sort-item".into(), label_index as u64))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::FONT_SMALL))
                    .when(selected, |d| {
                        d.text_color(theme::ACCENT).font_weight(FontWeight::SEMIBOLD)
                    })
                    .when(!selected, |d| d.text_color(theme::TEXT_DIM))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let this_store = self.store.clone();
                        let mode = *mode;
                        move |_, _, cx| {
                            this_store.update(cx, |s, cx| s.set_sort(mode, cx));
                        }
                    })
                    .child(*label),
            );
        }
        list.into_any_element()
    }

    fn render_totals(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let store = self.store.read(cx);
        let total = store.repos.len();
        let changes: u32 = store.repos.iter().map(|r| r.git_modified_count).sum();
        let prs: u32 = store.repos.iter().map(|r| r.github_pr_count).sum();
        let issues: u32 = store.repos.iter().map(|r| r.github_issue_count).sum();
        drop(store);

        let chip = |label: SharedString, color: gpui::Rgba| {
            div()
                .rounded_md()
                .bg(theme::CHIP)
                .px_2()
                .py_0p5()
                .text_size(px(theme::FONT_SMALL))
                .text_color(color)
                .child(label)
        };
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .pb_2()
            .child(chip(format!("{total} repositories").into(), theme::TEXT_MUTED))
            .child(chip(format!("{changes} changes").into(), theme::AMBER_BRIGHT))
            .child(chip(format!("{prs} pull requests").into(), theme::PURPLE_BRIGHT))
            .child(chip(format!("{issues} issues").into(), theme::BLUE_BRIGHT))
            .into_any_element()
    }
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
            if settings.repos.enable_azure_dev_ops {
                if col_ix == ix {
                    return ColKind::Azure;
                }
                ix += 1;
            }
            if col_ix == ix {
                ColKind::Activity
            } else {
                ColKind::Activity
            }
        }
    }
}

pub struct ReposDelegate {
    pub store: Entity<Store>,
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
    fn columns_count(&self, cx: &App) -> usize {
        let settings = self.store.read(cx).settings();
        6 + settings.repos.enable_git_hub as usize + settings.repos.enable_azure_dev_ops as usize
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.store.read(cx).projected.len()
    }

    fn column(&self, col_ix: usize, cx: &App) -> Column {
        let settings = self.store.read(cx).settings();
        match column_kind(col_ix, &settings) {
            ColKind::Star => {
                Column::new("star", "").width(px(36.)).resizable(false).text_center()
            }
            ColKind::Name => Column::new("name", "Repository").width(px(260.)).sortable(),
            ColKind::Shortcuts => {
                Column::new("shortcuts", "").width(px(96.)).resizable(false)
            }
            ColKind::Branch => Column::new("branch", "Branch").width(px(130.)).resizable(false),
            ColKind::Changes => Column::new("changes", "Changes").width(px(96.)).text_center().sortable(),
            ColKind::Github => Column::new("github", "GitHub").width(px(120.)).text_center().sortable(),
            ColKind::Azure => {
                Column::new("azure", "Azure DevOps").width(px(110.)).text_center().resizable(false)
            }
            ColKind::Activity => Column::new("activity", "Last activity").width(px(104.)).sortable(),
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
                    .size(px(22.))
                    .on_click({
                        let store = self.store.clone();
                        let folder = folder.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.toggle_favorite(&folder, cx));
                        }
                    })
                    .child(icon(
                        "icons/icon-star.svg",
                        px(theme::ICON_SMALL),
                        if favorite { theme::GOLD_BRIGHT } else { theme::STAR_MUTED },
                    ))
                    .into_any_element()
            }
            ColKind::Name => div()
                .flex()
                .flex_col()
                .py_1()
                .child(
                    div()
                        .text_size(px(theme::FONT_BODY))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::TEXT)
                        .child(repo.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(theme::FONT_SMALL - 1.))
                        .text_color(theme::TEXT_FAINT)
                        .child(folder.clone()),
                )
                .into_any_element(),
            ColKind::Shortcuts => shortcuts_cell(&repo, &settings, row_ix),
            ColKind::Branch => branch_pill(repo.git_branch_name.as_deref()),
            ColKind::Changes => changes_cell(&repo),
            ColKind::Github => github_cell(&repo),
            ColKind::Azure => azure_cell(&repo, &self.store, cx),
            ColKind::Activity => div()
                .text_size(px(theme::FONT_SMALL))
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
            (store.scanning, !store.search.trim().is_empty() || store.tag_filter.is_some(), has_folders)
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
            .text_size(px(theme::FONT_BODY))
            .text_color(theme::TEXT_MUTED)
            .child(text)
            .into_any_element()
    }
}

fn branch_pill(branch: Option<&str>) -> gpui::AnyElement {
    let Some(branch) = branch.filter(|b| !b.is_empty()) else {
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
        .text_size(px(theme::FONT_SMALL - 1.))
        .font_family(gpui::SharedString::from(theme::code_font()))
        .text_color(color)
        .child(branch.to_string())
        .into_any_element()
}

fn changes_cell(repo: &RepoEntry) -> gpui::AnyElement {
    if repo.git_modified_count == 0 && repo.git_to_push_count == 0 && repo.git_to_pull_count == 0 {
        if !repo.git_status_loaded {
            return div()
                .text_size(px(theme::FONT_SMALL))
                .text_color(theme::TEXT_FAINT)
                .child("…")
                .into_any_element();
        }
        return div()
            .text_size(px(theme::FONT_SMALL))
            .text_color(theme::TEXT_FAINT)
            .child("clean")
            .into_any_element();
    }
    let mut row = div().flex().flex_row().items_center().justify_center().gap_1().text_size(px(theme::FONT_SMALL));
    if repo.git_modified_count > 0 {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::AMBER_BRIGHT))
                .px_1p5()
                .text_color(theme::AMBER_BRIGHT)
                .child(format!("{}", repo.git_modified_count)),
        );
    }
    if repo.git_to_push_count > 0 {
        row = row.child(div().text_color(theme::SUCCESS_BRIGHT).child(format!("↑{}", repo.git_to_push_count)));
    }
    if repo.git_to_pull_count > 0 {
        row = row.child(div().text_color(theme::BLUE_BRIGHT).child(format!("↓{}", repo.git_to_pull_count)));
    }
    row.into_any_element()
}

fn github_cell(repo: &RepoEntry) -> gpui::AnyElement {
    if repo.github_pr_count == 0 && repo.github_issue_count == 0 {
        return div()
            .text_size(px(theme::FONT_SMALL))
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
        .text_size(px(theme::FONT_SMALL));
    if repo.github_pr_count > 0 {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::PURPLE_BRIGHT))
                .px_1p5()
                .text_color(theme::PURPLE_BRIGHT)
                .child(format!("PR {}", repo.github_pr_count)),
        );
    }
    if repo.github_issue_count > 0 {
        row = row.child(
            div()
                .rounded_md()
                .bg(theme::tint(theme::BLUE_BRIGHT))
                .px_1p5()
                .text_color(theme::BLUE_BRIGHT)
                .child(format!("◉ {}", repo.github_issue_count)),
        );
    }
    row.into_any_element()
}

fn azure_cell(repo: &RepoEntry, store: &Entity<Store>, cx: &App) -> gpui::AnyElement {
    let folder = repo.folder_path.to_string_lossy().to_string();
    let Some(snapshot) = store.read(cx).azure_by_folder.get(&folder) else {
        return div()
            .text_size(px(theme::FONT_SMALL))
            .text_color(theme::TEXT_FAINT)
            .child("·")
            .into_any_element();
    };
    if snapshot.pipelines.is_empty() && snapshot.pulls.is_empty() && snapshot.work_items.is_empty() {
        return div()
            .text_size(px(theme::FONT_SMALL))
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
        .text_size(px(theme::FONT_SMALL));
    // the newest run's state is the repo's pipeline state
    if snapshot.pipelines.first().is_some() {
        let (glyph, color) = if snapshot.pipeline_running() {
            ("◐", theme::BLUE_BRIGHT)
        } else if snapshot.pipeline_failed() {
            ("✗", theme::DANGER)
        } else if snapshot.pipeline_ok() {
            ("✓", theme::SUCCESS_BRIGHT)
        } else {
            ("·", theme::TEXT_MUTED)
        };
        row = row.child(div().text_color(color).child(glyph));
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

/// Per-row launch buttons: open folder, VS Code, terminal. Purely visibility
/// switches from settings — a configured-but-missing executable reports at
/// launch time. Each click runs the desktop opener directly (fire and forget).
fn shortcuts_cell(repo: &RepoEntry, settings: &AppSettings, row_ix: usize) -> gpui::AnyElement {
    use devtools_core::terminal;
    let folder = repo.folder_path.to_string_lossy().to_string();
    let mut row = div().flex().flex_row().items_center().gap_1();

    let button = |row_ix: usize, kind: &'static str, icon_path: &'static str, launch: Box<dyn Fn() -> Result<(), String>>| {
        div()
            .id((kind, row_ix))
            .cursor_pointer()
            .rounded_md()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .hover(|s| s.bg(theme::HOVER))
            .on_click(move |_, _, _| {
                if let Err(err) = launch() {
                    tracing::warn!("launch failed: {err}");
                }
            })
            .child(icon(icon_path, px(theme::ICON_SMALL), theme::TEXT_MUTED))
    };

    row = row.child(button(
        row_ix,
        "open-folder",
        "icons/icon-folder.svg",
        Box::new({
            let folder = folder.clone();
            move || terminal::open_folder(&folder)
        }),
    ));
    if settings.repos.enable_vscode {
        row = row.child(button(
            row_ix,
            "open-vscode",
            "icons/icon-code.svg",
            Box::new({
                let folder = folder.clone();
                let exe = settings.repos.vscode_executable.clone();
                let profile = settings.repos.vscode_profile.clone();
                move || terminal::open_in_vscode(&folder, &exe, &profile)
            }),
        ));
    }
    if settings.repos.enable_terminal {
        row = row.child(button(
            row_ix,
            "open-terminal",
            "icons/icon-terminal.svg",
            Box::new({
                let folder = folder.clone();
                let term = settings.repos.terminal_executable.clone();
                move || terminal::open_terminal(&folder, &term)
            }),
        ));
    }
    if settings.repos.enable_zcode {
        row = row.child(button(
            row_ix,
            "open-zcode",
            "icons/icon-agent-sparkle.svg",
            Box::new({
                let folder = folder.clone();
                let term = settings.repos.terminal_executable.clone();
                let zcode = settings.repos.zcode_executable.clone();
                move || terminal::launch_in_terminal(&folder, &zcode, &term)
            }),
        ));
    }
    row.into_any_element()
}
