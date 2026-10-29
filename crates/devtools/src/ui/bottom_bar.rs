//! The bottom bar: the selected repo's workspace — a tab strip (Overview /
//! Changes / Pull Requests / Issues / Pipelines) above the active panel, with
//! the header shape: star, name, branch pill (click copies), tags row,
//! refresh + kebab + close on the right. The bar itself is store-driven; only
//! ephemeral panel state (kebab menu open, add-tag field) lives here.

use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::changes::ChangesView;
use crate::ui::icon;
use crate::ui::A11y;
use crate::ui::azure_tab::AzureView;
use crate::ui::overview::OverviewView;
use crate::ui::pr_issues::ItemListView;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::Sizable as _;
use gpui::prelude::*;
use gpui::{canvas, deferred, div, px, App, ClipboardItem, Context, Entity, FontWeight, MouseButton, SharedString, Window};

type MenuAction = Box<dyn Fn(&mut App)>;

pub struct BottomBar {
    pub store: Entity<Store>,
    changes_view: Entity<ChangesView>,
    overview_view: Entity<OverviewView>,
    pr_view: Entity<ItemListView>,
    issue_view: Entity<ItemListView>,
    azure_view: Entity<AzureView>,
    kebab_open: bool,
    /// First click on "Remove from repositories" arms the item; the second
    /// click inside the open menu removes the repo. Reset whenever the menu
    /// closes.
    remove_armed: bool,
    add_tag: Entity<InputState>,
    /// The "+ Add tag" chip swaps for the inline input once pressed.
    add_tag_open: bool,
    /// live splitter drag: (cursor y at grab, bar height at grab)
    splitter_drag: std::rc::Rc<std::cell::Cell<Option<(gpui::Pixels, f32)>>>,
}

impl BottomBar {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let changes_view = cx.new(|cx| ChangesView::new(store.clone(), window, cx));
        let overview_view = cx.new(|cx| OverviewView::new(store.clone(), window, cx));
        let pr_view = cx.new(|_| ItemListView { store: store.clone(), pull_requests: true });
        let issue_view = cx.new(|_| ItemListView { store: store.clone(), pull_requests: false });
        let azure_view = cx.new(|_| AzureView { store: store.clone() });
        let add_tag = cx.new(|cx| InputState::new(window, cx).placeholder("New tag name"));
        Self {
            store,
            changes_view,
            overview_view,
            pr_view,
            issue_view,
            azure_view,
            kebab_open: false,
            remove_armed: false,
            add_tag,
            add_tag_open: false,
            splitter_drag: std::rc::Rc::new(std::cell::Cell::new(None)),
        }
    }

    /// Dismisses the bar's transient popups — the kebab menu, the armed
    /// remove, and the Changes tab's branch dropdown — from an outside click
    /// or Escape. Returns whether anything was actually open so the window
    /// can stop propagating.
    pub fn dismiss_kebab(&mut self, cx: &mut Context<Self>) -> bool {
        let branch_closed = self
            .changes_view
            .update(cx, ChangesView::dismiss_branch_menu);
        if self.kebab_open || self.remove_armed || branch_closed {
            self.kebab_open = false;
            self.remove_armed = false;
            cx.notify();
            true
        } else {
            false
        }
    }

    /// The bar's top-edge divider: drag to resize the panel (one shared
    /// height, 260–780 like the .NET bar; whole pixels only).
    fn render_splitter(&self) -> gpui::AnyElement {
        let store_handle = self.store.clone();
        let drag_state = self.splitter_drag.clone();
        let drag_for_paint = self.splitter_drag.clone();
        let store_for_paint = self.store.clone();
        div()
            .id("bar-splitter")
            .role(gpui::Role::Splitter)
            .aria_label("Resize panel")
            .aria_orientation(gpui::Orientation::Vertical)
            .relative()
            .w_full()
            .h(px(6.))
            .cursor_row_resize()
            .hover(|s| s.bg(theme::HOVER))
            .on_mouse_down(MouseButton::Left, move |event: &gpui::MouseDownEvent, _, cx| {
                let start_height = store_handle.read(cx).bar_height;
                drag_state.set(Some((event.position.y, start_height)));
                cx.stop_propagation();
            })
            .child(
                canvas(
                    // this gpui rev: on_mouse_event must register from the
                    // paint closure, prepaint panics on it
                    |_, _, _| {},
                    move |_, _, window, _| {
                        if drag_for_paint.get().is_some() {
                            let drag = drag_for_paint.clone();
                            let store = store_for_paint.clone();
                            window.on_mouse_event(
                                move |event: &gpui::MouseMoveEvent, phase, _window, cx| {
                                    if phase != gpui::DispatchPhase::Bubble {
                                        return;
                                    }
                                    let Some((start_y, start_height)) = drag.get() else { return };
                                    match event.pressed_button {
                                        Some(MouseButton::Left) => {
                                            // top edge follows the cursor: up = taller
                                            let delta = (event.position.y - start_y).as_f32();
                                            let value = (start_height - delta).round().clamp(260., 780.);
                                            store.update(cx, |s, cx| {
                                                if (s.bar_height - value).abs() >= 0.5 {
                                                    s.bar_height = value;
                                                    cx.notify();
                                                }
                                            });
                                        }
                                        _ => {
                                            drag.set(None);
                                            cx.stop_propagation();
                                        }
                                    }
                                },
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }

    fn copy_text(text: String, cx: &mut App) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

impl Render for BottomBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let Some(entry) = self.store.read(cx).selected_entry().cloned() else {
            return div().into_any_element();
        };
        let store_snapshot = self.store.read(cx);
        let selected_tab = store_snapshot.selected_tab;
        let bar_height = store_snapshot.bar_height;
        let settings = store_snapshot.settings();
        let folder = entry.folder_path.to_string_lossy().to_string();
        let github_url = store_snapshot
            .github_details
            .as_ref()
            .and_then(|d| d.url.clone());
        // the stored height is the user's preference; the rendered height
        // also has to leave the repos list enough room in short windows
        let max_height = (window.viewport_size().height.as_f32()
            - theme::TITLEBAR_H
            - 180.)
            .max(260.);
        let bar_height = bar_height.min(max_height);

        // ---- tabs (settings gates; the Changes tab carries the modified count) ----
        let mut tabs = div().flex().flex_row().items_end().gap_5();
        let modified = entry.git_modified_count;
        for tab in [
            BottomBarTab::Overview,
            BottomBarTab::Changes,
            BottomBarTab::PullRequests,
            BottomBarTab::Issues,
            BottomBarTab::Azure,
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
            // .NET bar tabs: plain text with an accent underline on the active
            // one; Changes shows the modified-file count beside its label
            let chip = div()
                .id(SharedString::from(format!("tab-{}", tab.label())))
                .a11y_tab(tab.label(), active)
                .cursor_pointer()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .text_size(px(theme::font_small()))
                .when(active, |d| d.text_color(theme::TEXT).font_weight(FontWeight::MEDIUM))
                .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                .hover(|s| s.text_color(theme::TEXT))
                .on_click({
                    let store = self.store.clone();
                    move |_, _, cx| {
                        store.update(cx, |s, cx| s.set_tab(tab, cx));
                    }
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(tab.label())
                        .when(
                            tab == BottomBarTab::Changes && modified > 0,
                            |row| {
                                row.child(
                                    div()
                                        .text_size(px(theme::font_small()))
                                        .text_color(theme::TEXT_FAINT)
                                        .child(format!("{modified}")),
                                )
                            },
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(2.))
                        .rounded_full()
                        .when(active, |d| d.bg(theme::ACCENT)),
                );
            tabs = tabs.child(chip);
        }

        // ---- tag chips row (removable, each chip tinted in its .NET hue) + Add tag ----
        // The favorites chip leads at hue 0 (amber) whenever the repo is
        // starred, so the regular tags keep their .NET hues (green, orange,
        // ...) even though the favorites tag lives inside entry.tags.
        let mut tag_chips = div().flex().flex_row().items_center().flex_wrap().gap_2();
        let mut chip_index = 0usize;
        if entry.is_favorite() {
            let color = tag_color(chip_index);
            chip_index += 1;
            tag_chips = tag_chips.child(
                div()
                    .id("tag-chip-favorites")
                    .a11y_button("Remove tag favorites")
                    .cursor_pointer()
                    .rounded_lg()
                    .bg(theme::tint(color))
                    .px_2()
                    .py_0p5()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .text_size(px(theme::font_small()))
                    .text_color(color)
                    .hover(|s| s.opacity(0.85))
                    .on_click({
                        let store = self.store.clone();
                        let folder = folder.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.toggle_favorite(&folder, cx));
                        }
                    })
                    .child("favorites")
                    .child(div().text_color(theme::TEXT_FAINT).child("×")),
            );
        }
        for tag in entry
            .tags
            .clone()
            .into_iter()
            .filter(|tag| !tag.eq_ignore_ascii_case("favorites"))
            // the archived marker is managed by the kebab's Archive item, not
            // a removable chip
            .filter(|tag| !tag.eq_ignore_ascii_case(devtools_core::repo_store::ARCHIVED_TAG))
        {
            let color = tag_color(chip_index);
            chip_index += 1;
            tag_chips = tag_chips.child(
                div()
                    .id(SharedString::from(format!("tag-chip-{tag}")))
                    .a11y_button(format!("Remove tag {tag}"))
                    .cursor_pointer()
                    .rounded_lg()
                    .bg(theme::tint(color))
                    .px_2()
                    .py_0p5()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .text_size(px(theme::font_small()))
                    .text_color(color)
                    .hover(|s| s.opacity(0.85))
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
        let add_tag_row = if self.add_tag_open {
            div()
                .id("add-tag-editor")
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .w(px(120.))
                        .text_size(px(theme::font_small() - 1.))
                        .child(Input::new(&tag_input).small()),
                )
                .child(
                    div()
                        .id("add-tag")
                        .a11y_button("Add tag")
                        .cursor_pointer()
                        .rounded_md()
                        .px_1p5()
                        .py_0p5()
                        .text_size(px(theme::font_small() - 1.))
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
                            }
                        })
                        .child("Add"),
                )
                .child(
                    div()
                        .id("add-tag-cancel")
                        .a11y_button("Cancel adding tag")
                        .cursor_pointer()
                        .rounded_md()
                        .px_1p5()
                        .py_0p5()
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_FAINT)
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.add_tag_open = false;
                            cx.notify();
                        }))
                        .child("×"),
                )
        } else {
            div()
                .id("add-tag-toggle")
                .a11y_button("Add a tag")
                .cursor_pointer()
                .rounded_md()
                .px_1p5()
                .py_0p5()
                .flex()
                .items_center()
                .gap_1()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_DIM)
                .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.add_tag_open = true;
                    this.add_tag.update(cx, |input, cx| input.focus(window, cx));
                    cx.notify();
                }))
                .child("+")
                .child("Add tag")
        };
        let kebab_store = self.store.clone();
        let kebab_open = self.kebab_open;
        let mut header = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_4()
            .py_1p5()
            .min_h(px(44.))
            .relative()
            .debug_selector(|| "bar-header".into())
            .child(
                // favorite star
                div()
                    .id("bar-star")
                    .a11y_toggle(
                        if entry.is_favorite() { "Remove from favorites" } else { "Add to favorites" },
                        entry.is_favorite(),
                    )
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::HOVER))
                    .child(icon(
                        "icons/icon-star.svg",
                        px(theme::icon_small()),
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
                    .flex_shrink_0()
                    .text_size(px(theme::font_body() + 1.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(entry.name.clone()),
            )
            .when_some(entry.git_branch_name.clone(), |row, branch| {
                row.child(
                    div()
                        .id("bar-branch")
                        .a11y_button(format!("Copy branch name {branch}"))
                        .cursor_pointer()
                        .flex_shrink_0()
                        .rounded_md()
                        .bg(theme::CHIP)
                        .px_1p5()
                        .py_0p5()
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_MUTED)
                        .hover(|s| s.text_color(theme::TEXT))
                        .on_click({
                            let store = self.store.clone();
                            let branch = branch.clone();
                            move |_, _, cx| {
                                Self::copy_text(branch.clone(), cx);
                                store.update(cx, |s, cx| {
                                    s.notify_success("Branch name copied");
                                    cx.notify();
                                });
                            }
                        })
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .font_family(SharedString::from(theme::code_font()))
                                .child(icon(
                                    "icons/icon-source-branch.svg",
                                    px(theme::icon_small() - 3.),
                                    theme::TEXT_MUTED,
                                ))
                                .child(branch.clone()),
                        ),
                )
            })
            // tags ride the identity row: no separate strip, one header
            .child(div().mx_1().h(px(14.)).w(px(1.)).bg(theme::HAIRLINE))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .child(tag_chips)
                    .child(add_tag_row),
            )
            .when_some(self.store.read(cx).last_git_error.clone(), |row, error| {
                row.child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::DANGER)
                        .max_w(px(240.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(error),
                )
            })
            // refresh panel — the .NET bar's bordered pill buttons
            .child(
                div()
                    .id("bar-refresh")
                    .a11y_button("Refresh panel")
                    .cursor_pointer()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.border_color(theme::TEXT_FAINT))
                    .on_click({
                        let store = self.store.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.refresh_panel(cx));
                        }
                    })
                    .child(icon("icons/icon-refresh.svg", px(theme::icon_small()), theme::TEXT_MUTED)),
            )
            // kebab menu
            .child(
                div()
                    .id("bar-kebab")
                    .a11y_button("More actions")
                    .cursor_pointer()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::HAIRLINE)
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .debug_selector(|| "bar-kebab".into())
                    .hover(|s| s.border_color(theme::TEXT_FAINT))
                    // keep the window's outside-click dismissal from seeing
                    // this press: it must toggle, not close-then-reopen
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click({
                        let this = cx.entity();
                        move |_, _, cx| {
                            this.update(cx, |bar, cx| {
                                bar.kebab_open = !bar.kebab_open;
                                bar.remove_armed = false;
                                cx.notify();
                            });
                        }
                    })
                    .child(icon("icons/icon-ellipsis-vertical.svg", px(theme::icon_small()), theme::TEXT_MUTED)),
            )
            .child(
                div()
                    .id("close-bar")
                    .a11y_button("Close panel")
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
                        px(theme::icon_small()),
                        theme::TEXT_MUTED,
                    )),
            );

        if kebab_open {
            // deferred: the header paints before the tabs row and panel —
            // without it those later siblings paint over the open menu and
            // its lower items vanish behind the Overview tiles
            header = header.child(deferred(
                self.render_kebab_menu(github_url.clone(), folder.clone(), cx),
            ));
        }

        div()
            .h(px(bar_height))
            .flex()
            .flex_col()
            .border_t_1()
            .border_color(theme::HAIRLINE)
            .debug_selector(|| "bar-root".into())
            // the strip itself is the raised surface and the panels inside it
            // are the cards
            .bg(theme::CARD)
            .child(self.render_splitter())
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_end()
                    .px_4()
                    .pt_1()
                    .border_b_1()
                    .border_color(theme::BORDER_LIGHT)
                    .child(tabs),
            )
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
            .id("kebab-menu")
            .role(gpui::Role::Menu)
            .aria_label("Repository actions")
            .absolute()
            .right(px(12.))
            .top(px(46.))
            .w(px(210.))
            .rounded_md()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .shadow_md()
            .py_1()
            // clicks inside the menu must not reach the window's outside-click
            // dismissal; items close the menu themselves
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .debug_selector(|| "kebab-menu".into());
        let make_item = |label: &'static str, action: MenuAction| -> gpui::AnyElement {
            let this = this.clone();
            div()
                .id(SharedString::from(format!("kebab-{label}")))
                .debug_selector(move || format!("kebab-{label}"))
                .a11y_menu_item(label)
                .cursor_pointer()
                .px_3()
                .py_1p5()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_DIM)
                .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    action(cx);
                    this.update(cx, |bar, cx| {
                        bar.kebab_open = false;
                        bar.remove_armed = false;
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
                        s.notify_success("GitHub URL copied");
                        cx.notify();
                    });
                })));
        }
        let folder_copy = folder.clone();
        let store_folder = store.clone();
        let folder_archive = folder.clone();
        let store_archive = store.clone();
        let archive_name = self.store.read(cx).selected_entry().map(|e| e.name.clone()).unwrap_or_default();
        let folder_remove = folder.clone();
        let store_remove = store.clone();
        let remove_armed = self.remove_armed;
        menu = menu
            .child(make_item("Copy folder path", Box::new(move |cx: &mut App| {
                Self::copy_text(folder_copy.clone(), cx);
                store_folder.update(cx, |s, cx| {
                    s.notify_success("Folder path copied");
                    cx.notify();
                });
            })))
            // reversible, so no arming like the remove below
            .child(make_item("Archive repository", Box::new(move |cx: &mut App| {
                store_archive.update(cx, |s, cx| {
                    s.set_repo_archived(&folder_archive, true, cx);
                    s.notify_success(format!("Archived {archive_name}"));
                    cx.notify();
                });
            })))
            .child(div().h(px(1.)).mx_2().my_1().bg(theme::HAIRLINE))
            .child({
                let this = this.clone();
                div()
                    .id("kebab-remove")
                    .a11y_button(if remove_armed {
                        "Confirm remove from repositories"
                    } else {
                        "Remove from repositories"
                    })
                    .cursor_pointer()
                    .px_3()
                    .py_1p5()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .text_size(px(theme::font_small()))
                    .text_color(if remove_armed { theme::DANGER } else { theme::TEXT_DIM })
                    .when(remove_armed, |d| d.bg(theme::tint(theme::DANGER)))
                    .hover(|s| {
                        s.bg(theme::tint(theme::DANGER))
                            .text_color(theme::DANGER)
                    })
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        if remove_armed {
                            let folder = folder_remove.clone();
                            store_remove.update(cx, |s, cx| {
                                s.selected_repo = Some(folder);
                                s.remove_from_repositories(cx);
                            });
                            this.update(cx, |bar, cx| {
                                bar.kebab_open = false;
                                bar.remove_armed = false;
                                cx.notify();
                            });
                        } else {
                            // destructive action: require a second click on the
                            // now-highlighted item; any outside click disarms
                            this.update(cx, |bar, cx| {
                                bar.remove_armed = true;
                                cx.notify();
                            });
                        }
                    })
                    .child(if remove_armed {
                        "Remove from repositories?"
                    } else {
                        "Remove from repositories"
                    })
                    .when(remove_armed, |d| {
                        d.child(
                            div()
                                .text_size(px(theme::font_small() - 2.))
                                .text_color(theme::DANGER)
                                .child("Click again to confirm"),
                        )
                    })
            });
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

/// The .NET bar paints each tag chip in a hue picked by the tag's position on
/// the repo — favorites amber, then green, orange, ... cycling (measured off
/// the original: favorites `#F0A03C`, llm `#3EBE48`, rust `#EB8C2E`).
fn tag_color(index: usize) -> gpui::Rgba {
    const PALETTE: [gpui::Rgba; 6] = [
        theme::AMBER_BRIGHT,
        theme::SUCCESS_BRIGHT,
        theme::WARN,
        theme::BLUE_BRIGHT,
        theme::PURPLE_BRIGHT,
        theme::GOLD_BRIGHT,
    ];
    PALETTE[index % PALETTE.len()]
}
