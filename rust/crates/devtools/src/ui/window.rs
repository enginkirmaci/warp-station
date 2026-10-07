//! The main window: custom titlebar + page content. Hosts the Repos page;
//! Settings swaps content and Notes opens a floating window (later waves).

use crate::store::Store;
use crate::theme;
use crate::ui::bottom_bar::BottomBar;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{div, px, App, Context, Entity, FontWeight, MouseButton, Render, Window, WindowControlArea};

/// Which page fills the window body (the bottom bar rides only with Repos).
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Repos,
    Notes,
    Settings,
}

pub struct MainWindow {
    pub store: Entity<Store>,
    page: Page,
    repos_page: Entity<crate::ui::repos::ReposPage>,
    notes_page: Entity<crate::ui::notes::NotesPage>,
    settings_page: Entity<crate::ui::settings::SettingsPage>,
    bottom_bar: Entity<BottomBar>,
}

impl MainWindow {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let repos_page = cx.new(|cx| crate::ui::repos::ReposPage::new(store.clone(), window, cx));
        let notes_page = cx.new(|cx| crate::ui::notes::NotesPage::new(store.clone(), window, cx));
        let settings_page = cx.new(|cx| crate::ui::settings::SettingsPage::new(store.clone(), window, cx));
        let bottom_bar = cx.new(|cx| BottomBar::new(store.clone(), window, cx));
        // the bottom bar is store-driven; the window must repaint with it
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self { store, page: Page::Repos, repos_page, notes_page, settings_page, bottom_bar }
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::BACKGROUND)
            .text_color(theme::TEXT)
            .font_family(theme::ui_font())
            .child(self.render_titlebar(window, cx))
            .child(self.render_body(window, cx))
            .relative();
        if let Some(ask) = self.render_wand_ask(cx) {
            root = root.child(ask);
        }
        root
    }
}

impl MainWindow {
    /// The opencode permission popup: allow once / allow all / reject, with
    /// the auto-reject countdown.
    fn render_wand_ask(&mut self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let (wand_id, ask_id, title, secs) = self.store.read(cx).wand_ask.clone()?;
        let store = self.store.clone();
        let store_all = self.store.clone();
        let store_reject = self.store.clone();
        let _ = (&wand_id, &ask_id);
        Some(
            div()
                .id("wand-ask-overlay")
                .absolute()
                .inset_0()
                .occlude()
                .bg(gpui::black().opacity(0.45))
                .flex()
                .items_start()
                .justify_center()
                .pt(px(90.))
                .child(
                    div()
                        .id("wand-ask-card")
                        .occlude()
                        .min_w(px(420.))
                        .rounded_xl()
                        .border_1()
                        .border_color(theme::tint(theme::WARN))
                        .bg(theme::POPUP)
                        .shadow_lg()
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .on_click(|_, _, _| {})
                        .child(
                            div()
                                .text_size(px(theme::FONT_TITLE))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("OpenCode needs permission"),
                        )
                        .child(
                            div()
                                .text_size(px(theme::FONT_SMALL))
                                .text_color(theme::TEXT_DIM)
                                .child(title.clone()),
                        )
                        .child(
                            div().flex().flex_row().justify_end().gap_2()
                                .child(ask_button("Reject", theme::DANGER, move |_, _, cx| {
                                    store_reject.update(cx, |s, cx| s.respond_wand("reject", cx));
                                }))
                                .child(ask_button("Allow all", theme::ACCENT, move |_, _, cx| {
                                    store_all.update(cx, |s, cx| s.respond_wand("always", cx));
                                }))
                                .child(ask_button("Allow once", theme::SUCCESS_BRIGHT, move |_, _, cx| {
                                    store.update(cx, |s, cx| s.respond_wand("once", cx));
                                })),
                        )
                        .child(
                            div()
                                .text_size(px(theme::FONT_SMALL - 1.))
                                .text_color(theme::TEXT_FAINT)
                                .child(format!("auto-reject in {secs}s")),
                        ),
                )
                .into_any_element(),
        )
    }
}

fn ask_button(
    label: &str,
    color: gpui::Rgba,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(gpui::SharedString::from(format!("wand-ask-{label}")))
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::FONT_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::tint(color))
        .text_color(color)
        .hover(|s| s.opacity(0.85))
        .on_click(on_click)
        .into_any_element()
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
            .child({
                let on_settings = self.page == Page::Settings;
                div()
                    .id("settings-toggle")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(on_settings, |d| d.bg(theme::tint(theme::ACCENT)))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.page = if this.page == Page::Settings { Page::Repos } else { Page::Settings };
                        cx.notify();
                    }))
                    .child(icon(
                        "icons/icon-cog.svg",
                        px(theme::ICON_MEDIUM),
                        if on_settings { theme::ACCENT } else { theme::TEXT_MUTED },
                    ))
            })
            .child({
                let store_page = self.page;
                let on_notes = store_page == Page::Notes;
                div()
                    .id("notes-toggle")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(on_notes, |d| d.bg(theme::tint(theme::ACCENT)))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.page = if this.page == Page::Notes { Page::Repos } else { Page::Notes };
                        cx.notify();
                    }))
                    .child(icon(
                        "icons/icon-agent-sparkle.svg",
                        px(theme::ICON_MEDIUM),
                        if on_notes { theme::ACCENT } else { theme::TEXT_MUTED },
                    ))
            })
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

    fn render_body(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> gpui::AnyElement {
        match self.page {
            Page::Repos => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(div().flex_1().min_h_0().child(self.repos_page.clone()))
                .child(self.bottom_bar.clone())
                .into_any_element(),
            Page::Notes => div()
                .flex_1()
                .min_h_0()
                .child(self.notes_page.clone())
                .into_any_element(),
            Page::Settings => div()
                .flex_1()
                .min_h_0()
                .child(self.settings_page.clone())
                .into_any_element(),
        }
    }
}
