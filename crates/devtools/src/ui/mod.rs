//! ui module: shared helpers + the main window.

pub mod azure_tab;
pub mod bottom_bar;
pub mod changes;
pub mod clipboard_password;
pub mod dialogs;
pub mod diff;
pub mod formatters;
pub mod markdown_editor;
pub mod filtered_picker;
pub mod notes;
pub mod nuget;
pub mod overview;
pub mod pr_issues;
pub mod repos;
pub mod settings;
pub mod window;

use gpui::prelude::*;
use gpui::{div, px, FontWeight, Styled};

use crate::theme;

/// A tooltip body in the app's palette. GPUI hosts tooltip views in their own
/// layer, so this is a tiny view rather than an absolutely-positioned child.
struct TooltipText {
    text: gpui::SharedString,
}

impl Render for TooltipText {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .rounded_md()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .px_2()
            .py_1()
            .text_size(px(theme::font_small() - 1.))
            .text_color(theme::TEXT_DIM)
            .shadow_md()
            .whitespace_nowrap()
            .child(self.text.clone())
    }
}

/// Tooltip builder for `.tooltip(ui::tooltip_text("…"))`.
pub fn tooltip_text(
    text: impl Into<gpui::SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| {
        let text = text.clone();
        cx.new(move |_| TooltipText { text }).into()
    }
}

/// Attaches a tooltip when the user has "Show tooltips" enabled. Every helper
/// takes an id'd element, so this stays a plain `Stateful<Div>` pass-through.
pub fn with_tooltip(
    element: gpui::Stateful<gpui::Div>,
    show: bool,
    text: impl Into<gpui::SharedString> + 'static,
) -> gpui::Stateful<gpui::Div> {
    if show {
        let text = text.into();
        element.tooltip(tooltip_text(text))
    } else {
        element
    }
}

/// A tinted svg icon.
pub fn icon(path: &str, size: gpui::Pixels, color: impl Into<gpui::Hsla>) -> gpui::Svg {
    // flex_shrink_0: an svg is a leaf with no intrinsic size, so the default
    // flex-shrink can squeeze it to zero bounds in a tight container — gpui
    // then fails the render with "can't render at a zero size" on every repaint
    gpui::svg()
        .path(path)
        .size(size)
        .flex_shrink_0()
        .text_color(color.into())
}

/// The close hook every drawer shares: the host window routes its own
/// close path in via `with_on_close`.
pub type DrawerCloseHandler = std::rc::Rc<dyn Fn(&mut gpui::Window, &mut gpui::App)>;
pub type DrawerOnClose = Option<DrawerCloseHandler>;

/// The shared drawer title row: a title (with an optional one-line hint) and
/// a close button on the right. The close button only appears when the host
/// window wired a handler via the drawer's `with_on_close`.
pub fn drawer_header(
    title: &'static str,
    hint: &'static str,
    on_close: DrawerOnClose,
) -> gpui::AnyElement {
    let mut header = div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(theme::font_title()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::TEXT)
                        .child(title),
                )
                .child(div().flex_1())
                .when_some(on_close, |row, close| {
                    row.child(
                        div()
                            .id("drawer-close")
                            .cursor_pointer()
                            .rounded_md()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(move |_, window, cx| close(window, cx))
                            .child(icon(
                                "icons/icon-close.svg",
                                px(theme::icon_small()),
                                theme::TEXT_MUTED,
                            )),
                    )
                }),
        );
    if !hint.is_empty() {
        header = header.child(
            div()
                .text_size(px(theme::font_small() - 1.))
                .text_color(theme::TEXT_FAINT)
                .child(hint),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .pb_3()
        .border_b_1()
        .border_color(theme::BORDER_LIGHT)
        .child(header)
        .into_any_element()
}
