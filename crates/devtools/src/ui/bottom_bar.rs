//! The bottom bar: the selected repo's workspace — a tab strip (Overview /
//! Changes / Pull Requests / Issues / Pipelines) above the active panel, with
//! the header shape: star, name, branch pill (click copies), tags row,
//! refresh + kebab + close on the right. The bar itself is store-driven; only
//! ephemeral panel state (kebab menu open, add-tag field) lives here.

use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::changes::ChangesView;
use crate::ui::icon;
use crate::ui::azure_tab::AzureView;
use crate::ui::overview::OverviewView;
use crate::ui::pr_issues::ItemListView;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::Sizable as _;
use gpui::prelude::*;
use gpui::{div, px, App, ClipboardItem, Context, Entity, FontWeight, SharedString, Window};

type MenuAction = Box<dyn Fn(&mut App)>;

pub struct BottomBar {
    pub store: Entity<Store>,
    changes_view: Entity<ChangesView>,
    overview_view: Entity<OverviewView>,
    pr_view: Entity<ItemListView>,
    issue_view: Entity<ItemListView>,
    azure_view: Entity<AzureView>,
    kebab_open: bool,
    add_tag: Entity<InputState>,
}

impl BottomBar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let changes_view = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        let overview_view = cx.new(|cx| OverviewView::new(store.clone(), window, cx));
        let pr_view = cx.new(|_| ItemListView { store: store.clone(), pull_requests: true });
        let issue_view = cx.new(|_| ItemListView { store: store.clone(), pull_requests: false });
        let azure_view = cx.new(|_| AzureView { store: store.clone() });
        let add_tag = cx.new(|cx| InputState::new(window, cx).placeholder("New tag name"));
        Self { store, changes_view, overview_view, pr_view, issue_view, azure_view, kebab_open: false, add_tag }
    }

    fn copy_text(text: String, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

impl Render for BottomBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let Some(entry) = self.store.read(cx).selected_entry().cloned() else {
            return div().into_any_element();
        };
        let store_snapshot = self.store.read(cx);
        let selected_tab = store_snapshot.selected_tab;
        let settings = store_snapshot.settings();
        let folder = entry.folder_path.to_string_lossy().to_string();
        let github_url = store_snapshot
            .github_details
            .as_ref()
            .and_then(|d| d.url.clone());

        // ---- tabs (badges + settings gates) ----
        let mut tabs = div().flex().flex_row().gap_1();
        let modified = entry.git_modified_count;
        for (tab, badge) in [
            (BottomBarTab::Overview, 0u32),
            (BottomBarTab::Changes, modified),
            (
                BottomBarTab::PullRequests,
                if settings.repos.enable_git_hub { entry.github_pr_count } else { 0 },
            ),
            (
                BottomBarTab::Issues,
                if settings.repos.enable_git_hub { entry.github_issue_count } else { 0 },
            ),
            (
                BottomBarTab::Azure,
                if settings.repos.enable_azure_dev_ops { 1u32 } else { 0 },
            ),
        ] {
            if tab == BottomBarTab::PullRequests && !settings.repos.enable_git_hub {
                continue;
            }
            if tab == BottomBarTab::Issues && !settings.repos.enable_git_hub {
                continue;
            }
            if tab == BottomBarTab::Azure && !settings.repos.enable_azure_dev_ops {
                continue;
            }
            let active = tab == selected_tab;
            let mut chip = div()
                .id(SharedString::from(format!("tab-{}", tab.label())))
                .cursor_pointer()
                .rounded_md()
                .px_2p5()
                .py_1()
                .text_size(px(theme::FONT_SMALL))
                .when(active, |d| {
                    d.bg(theme::tint(theme::ACCENT))
                        .text_color(theme::ACCENT)
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                .hover(|s| s.opacity(0.85))
                .on_click({
                    let store = self.store.clone();
                    move |_, _, cx| {
                        store.update(cx, |s, cx| s.set_tab(tab, cx));
                    }
                })
                .child(tab.label());
            // Pipelines has no single badge (it is a section list); the rest
            // badge their count
            if badge > 0 && tab != BottomBarTab::Azure {
                chip = chip.child(
                    div()
                        .ml_1()
                        .px_1()
                        .rounded_full()
                        .bg(theme::tint(theme::ACCENT))
                        .text_size(px(theme::FONT_SMALL - 2.))
                        .child(format!("{badge}")),
                );
            }
            tabs = tabs.child(chip);
        }

        // ---- tag chips row (removable) + Add tag ----
        let mut tag_chips = div().flex().flex_row().items_center().flex_wrap().gap_1();
        for tag in entry.tags.clone() {
            if tag.eq_ignore_ascii_case("favorites") {
                continue;
            }
            tag_chips = tag_chips.child(
                div()
                    .id(SharedString::from(format!("tag-chip-{tag}")))
                    .cursor_pointer()
                    .rounded_md()
                    .bg(theme::CHIP)
                    .px_1p5()
                    .py_0p5()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::FONT_SMALL - 1.))
                    .text_color(theme::TEXT_DIM)
                    .hover(|s| s.text_color(theme::DANGER))
                    .on_click({
                        let store = self.store.clone();
                        let folder = folder.clone();
                        let tag = tag.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.toggle_repo_tag(&folder, &tag, cx));
                        }
                    })
                    .child(tag.clone())
                    .child(div().text_color(theme::TEXT_FAINT).child("×")),
            );
        }
        let tag_input = self.add_tag.clone();
        let add_tag_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                div()
                    .w(px(120.))
                    .text_size(px(theme::FONT_SMALL - 1.))
                    .child(Input::new(&tag_input).small()),
            )
            .child(
                div()
                    .id("add-tag")
                    .cursor_pointer()
                    .rounded_md()
                    .px_1p5()
                    .py_0p5()
                    .text_size(px(theme::FONT_SMALL - 1.))
                    .text_color(theme::ACCENT)
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let store = self.store.clone();
                        let add_tag = self.add_tag.clone();
                        let folder = folder.clone();
                        move |_, window, cx| {
                            let tag = add_tag.read(cx).value().to_string();
                            if tag.trim().is_empty() {
                                return;
                            }
                            store.update(cx, |s, cx| s.add_repo_tag(&folder, &tag, cx));
                            add_tag.update(cx, |input, cx| input.set_value(String::new(), window, cx));
                        }                    })
                    .child("+ Add tag"),
            );
        let tags_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .pb_1()
            .child(tag_chips)
            .child(add_tag_row);

        let kebab_store = self.store.clone();
        let kebab_open = self.kebab_open;
        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_1p5()
            .relative()
            .child(
                // favorite star
                div()
                    .id("bar-star")
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        "icons/icon-star.svg",
                        px(theme::ICON_SMALL),
                        if entry.is_favorite() { theme::GOLD_BRIGHT } else { theme::STAR_MUTED },
                    ))
                    .on_click({
                        let store = self.store.clone();
                        let folder = folder.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.toggle_favorite(&folder, cx));
                        }
                    }),
            )
            .child(
                div()
                    .text_size(px(theme::FONT_BODY))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(entry.name.clone()),
            )
            .when_some(entry.git_branch_name.clone(), |row, branch| {
                row.child(
                    div()
                        .id("bar-branch")
                        .cursor_pointer()
                        .rounded_md()
                        .bg(theme::CHIP)
                        .px_1p5()
                        .py_0p5()
                        .text_size(px(theme::FONT_SMALL - 1.))
                        .text_color(theme::TEXT_MUTED)
                        .hover(|s| s.text_color(theme::TEXT))
                        .on_click({
                            let store = self.store.clone();
                            let branch = branch.clone();
                            move |_, _, cx| {
                                Self::copy_text(branch.clone(), cx);
                                store.update(cx, |s, cx| {
                                    s.notice =
                                        Some(("Branch name copied".into(), std::time::Instant::now()));
                                    cx.notify();
                                });
                            }
                        })
                        .child(format!("⎇ {branch}")),
                )
            })
            .child(div().w(px(8.)))
            .child(tabs)
            .child(div().flex_1())
            .when_some(self.store.read(cx).last_git_error.clone(), |row, error| {
                row.child(
                    div()
                        .text_size(px(theme::FONT_SMALL))
                        .text_color(theme::DANGER)
                        .max_w(px(320.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(error),
                )
            })
            // refresh panel
            .child(
                div()
                    .id("bar-refresh")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let store = self.store.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.refresh_panel(cx));
                        }
                    })
                    .child(icon("icons/icon-refresh.svg", px(theme::ICON_SMALL), theme::TEXT_MUTED)),
            )
            // kebab menu
            .child(
                div()
                    .id("bar-kebab")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let this = cx.entity();
                        move |_, _, cx| {
                            this.update(cx, |bar, cx| {
                                bar.kebab_open = !bar.kebab_open;
                                cx.notify();
                            });
                        }
                    })
                    .child(div().text_color(theme::TEXT_MUTED).child("⋮")),
            )
            .child(
                div()
                    .id("close-bar")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(move |_, _, cx| {
                        kebab_store.update(cx, |s, cx| s.close_bar(cx));
                    })
                    .child(icon(
                        "icons/icon-window-close.svg",
                        px(theme::ICON_SMALL),
                        theme::TEXT_MUTED,
                    )),
            );

        if kebab_open {
            header = header.child(self.render_kebab_menu(github_url.clone(), folder.clone(), cx));
        }

        div()
            .h(px(440.))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::STRONG_BACKGROUND)
            .child(header)
            .child(tags_row)
            .child(div().flex_1().min_h_0().child(self.render_panel(selected_tab)))
            .into_any_element()
    }
}

impl BottomBar {
    fn render_kebab_menu(
        &mut self,
        github_url: Option<String>,
        folder: String,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let store = self.store.clone();
        let this = cx.entity();
        let mut menu = div()
            .absolute()
            .right(px(12.))
            .top(px(34.))
            .w(px(210.))
            .rounded_md()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .shadow_md()
            .py_1();
        let make_item = |label: &'static str, action: MenuAction| -> gpui::AnyElement {
            let this = this.clone();
            div()
                .id(SharedString::from(format!("kebab-{label}")))
                .cursor_pointer()
                .px_3()
                .py_1p5()
                .text_size(px(theme::FONT_SMALL))
                .text_color(theme::TEXT_DIM)
                .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    action(cx);
                    this.update(cx, |bar, cx| {
                        bar.kebab_open = false;
                        cx.notify();
                    });
                })
                .child(label)
                .into_any_element()
        };
        if let Some(url) = github_url.clone() {
            let url_open = url.clone();
            let url_copy = url.clone();
            let store_copy = store.clone();
            menu = menu
                .child(make_item("Open in GitHub", Box::new(move |_cx: &mut App| {
                    let url = url_open.clone();
                    let _ = devtools_core::terminal::open_folder(&url);
                })))
                .child(make_item("Copy GitHub URL", Box::new(move |cx: &mut App| {
                    Self::copy_text(url_copy.clone(), cx);
                    store_copy.update(cx, |s, cx| {
                        s.notice = Some(("GitHub URL copied".into(), std::time::Instant::now()));
                        cx.notify();
                    });
                })));
        }
        let folder_copy = folder.clone();
        let store_folder = store.clone();
        let folder_remove = folder.clone();
        let store_remove = store.clone();
        menu = menu
            .child(make_item("Copy folder path", Box::new(move |cx: &mut App| {
                Self::copy_text(folder_copy.clone(), cx);
                store_folder.update(cx, |s, cx| {
                    s.notice = Some(("Folder path copied".into(), std::time::Instant::now()));
                    cx.notify();
                });
            })))
            .child(make_item("Remove from repositories", Box::new(move |cx: &mut App| {
                let folder = folder_remove.clone();
                store_remove.update(cx, |s, cx| {
                    s.selected_repo = Some(folder);
                    s.remove_from_repositories(cx);
                });
            })));
        menu.into_any_element()
    }

    fn render_panel(&mut self, tab: BottomBarTab) -> gpui::AnyElement {
        match tab {
            BottomBarTab::Changes => self.changes_view.clone().into_any_element(),
            BottomBarTab::Overview => self.overview_view.clone().into_any_element(),
            BottomBarTab::PullRequests => self.pr_view.clone().into_any_element(),
            BottomBarTab::Issues => self.issue_view.clone().into_any_element(),
            BottomBarTab::Azure => self.azure_view.clone().into_any_element(),
        }
    }
}
