//! The main window: custom titlebar + page content. Hosts the Repos page;
//! Settings swaps content and Notes opens a floating window (later waves).

use crate::store::Store;
use crate::theme;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{div, px, App, Context, Entity, FontWeight, MouseButton, Render, Window, WindowControlArea};

pub struct MainWindow {
    pub store: Entity<Store>,
    repos_page: Entity<crate::ui::repos::ReposPage>,
}

impl MainWindow {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let repos_page = cx.new(|cx| crate::ui::repos::ReposPage::new(store.clone(), window, cx));
        // the bottom bar is store-driven; the window must repaint with it
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store, repos_page }
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::BACKGROUND)
            .text_color(theme::TEXT)
            .font_family(theme::ui_font())
            .child(self.render_titlebar(window, cx))
            .child(self.render_body(window, cx))
    }
}

fn window_button(
    id: &'static str,
    icon_path: &'static str,
    hover_bg: gpui::Hsla,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> impl gpui::IntoElement {
    div()
        .id(id)
        .cursor_pointer()
        .size(px(26.))
        .rounded_md()
        .flex()
        .items_center()
        .justify_center()
        .hover(move |s| s.bg(hover_bg))
        .on_click(on_click)
        .child(icon(icon_path, px(theme::ICON_MEDIUM), theme::TEXT_MUTED))
}

impl MainWindow {
    fn render_titlebar(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .h(px(38.))
            .flex()
            .items_center()
            .px_2()
            .gap_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                gpui_kit::base::GlobalState::suppress_text_selection(cx);
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(30.))
                    .child(icon("icons/icon-logo-mark.svg", px(theme::ICON_LARGE), theme::TEXT)),
            )
            .child(
                div()
                    .text_size(px(theme::FONT_TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Dev Tools"),
            )
            .child(
                div()
                    .id("titlebar-drag")
                    .flex_1()
                    .h_full()
                    // Windows drags via the hit-test path (this hitbox answers
                    // HTCAPTION); start_window_move is a no-op there. Linux
                    // keeps the explicit move call.
                    .window_control_area(WindowControlArea::Drag)
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.start_window_move();
                    }),
            )
            .child(window_button(
                "minimize-window",
                "icons/icon-window-minimize.svg",
                theme::HOVER,
                |_, window, _| window.minimize_window(),
            ))
            .child(window_button(
                "zoom",
                if window.is_maximized() {
                    "icons/icon-window-restore.svg"
                } else {
                    "icons/icon-window-maximize.svg"
                },
                theme::HOVER,
                cx.listener(|_, _, window, cx| {
                    window.zoom_window();
                    // the compositor toggles asynchronously — nudge a
                    // re-render so the maximize/restore glyph keeps up
                    cx.notify();
                }),
            ))
            .child(window_button(
                "close-window",
                "icons/icon-window-close.svg",
                theme::tint(theme::DANGER),
                |_, window, _| window.remove_window(),
            ))
            .into_any_element()
    }

    fn render_body(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(div().flex_1().min_h_0().child(self.repos_page.clone()))
            .child(self.render_bottom_bar(cx))
            .into_any_element()
    }

    /// Placeholder strip for the repo workspace bar — the tab strip lands with
    /// the Changes tab (wave 3).
    fn render_bottom_bar(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(entry) = self.store.read(cx).selected_entry().cloned() else {
            return div().into_any_element();
        };
        let store = self.store.clone();
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_1p5()
            .border_t_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::STRONG_BACKGROUND)
            .child(
                div()
                    .text_size(px(theme::FONT_BODY))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(entry.name.clone()),
            )
            .child(
                div()
                    .text_size(px(theme::FONT_SMALL))
                    .text_color(theme::TEXT_MUTED)
                    .child(entry.git_branch_name.clone().unwrap_or_else(|| "—".into())),
            )
            .child(div().flex_1())
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
                        store.update(cx, |s, cx| s.select_repo(None, cx));
                    })
                    .child(icon("icons/icon-window-close.svg", px(theme::ICON_SMALL), theme::TEXT_MUTED)),
            )
            .into_any_element()
    }
}
