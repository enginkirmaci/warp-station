//! The Repos page: the pane head (totals, scope, tag chips, sort, the toolbar
//! actions), the list and the detail rail beside it, and the key hints along
//! the bottom. Everything reads the store at render time, so filter, sort and
//! status changes land with the next frame.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::dialogs::{AddRepoDialog, CloneDialog, Dialog};
use crate::ui::icon;
use crate::ui::inspector::RepoMenu;
use warp_station_core::settings::RepoSortMode;
use gpui::prelude::*;
use gpui::{App, Context, Entity, FontWeight, MouseButton, Pixels, SharedString, Window, div, px};
use gpui_kit::component::input::{InputEvent, InputState};

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

pub struct ReposPage {
    pub store: Entity<Store>,
    pub dialog: Option<Dialog>,
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
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |this, store, cx| {
            store.update(cx, |s, cx| s.ensure_row_selected(cx));
            // the menu belongs to the repo it opened on
            let selected = store.read(cx).selected_repo.clone();
            if this
                .repo_menu
                .as_ref()
                .is_some_and(|menu| Some(&menu.folder) != selected.as_ref())
            {
                this.close_repo_menu(cx);
            }
            cx.notify();
        })
        .detach();
        store.update(cx, |s, cx| s.ensure_row_selected(cx));
        Self {
            store,
            dialog: None,
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
            let folder = this.store.read(cx).selected_repo.clone();
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
        let selected: Vec<warp_station_core::scan::ScannedRepo> = dialog
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
        self.ensure_tag_editor(window, cx);
        let list_w = window.viewport_size().width - px(crate::ui::inspector::RAIL_W);
        let toolbar = self.render_toolbar(list_w, cx);
        let sort_menu = self.render_sort_menu(cx);
        let mut page = {
            let store = self.store.read(cx);
            let list = crate::ui::repos_list::render(store, &self.store);
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
        // a dialog is a layer over the page
        if let Some(dialog) = &mut self.dialog {
            page = page.child(dialog.render(window, cx));
        }
        page
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
        let compact = viewport_width < px(530.) * theme::font_factor();
        let show_subtitle = !compact;
        let show_tag_chips = !compact;

        // Tag filters read as chips: raised when active, quiet otherwise (OR
        // semantics across the checked set); each carries its live count.
        // The export's chip row carries user tags that actually match
        // something: the derived flags (favourites/archived) have their own
        // scope segments and a zero-count chip is noise.
        let tag_counts: Vec<(String, usize)> = {
            let store = self.store.read(cx);
            tags.iter()
                .filter(|tag| {
                    !tag.eq_ignore_ascii_case(warp_station_core::repo_store::FAVORITES_TAG)
                        && !tag.eq_ignore_ascii_case(warp_station_core::repo_store::ARCHIVED_TAG)
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
                    .any(|t| t.eq_ignore_ascii_case(warp_station_core::repo_store::FAVORITES_TAG)),
                cx.listener(|this, _, _, cx| {
                    this.store.update(cx, |s, cx| {
                        s.toggle_tag_filter(warp_station_core::repo_store::FAVORITES_TAG, cx);
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

        let (sort_mode, sort_ascending) = {
            let store = self.store.read(cx);
            (store.sort_mode, store.sort_ascending)
        };
        let sort_button = div()
            .flex_shrink_0()
            // keep the window's outside-click dismissal from seeing this
            // press: it must toggle, not close-then-reopen
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(toolbar_action(
                "sort-button",
                format!("{} {}", sort_label(sort_mode), sort_arrow(sort_ascending)),
                None,
                false,
                cx.listener(|this, _, _, cx| this.toggle_sort_menu(cx)),
            ));

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
            .child(sort_button);

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
