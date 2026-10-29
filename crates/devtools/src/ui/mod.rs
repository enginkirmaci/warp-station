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
use gpui::{div, px, FontWeight, SharedString, Styled, Toggled};

use crate::theme;

/// gpui reports an accessibility node only for an id-ed element with an
/// explicit role, and a node without a label is invisible to AT-SPI clients —
/// so every hand-rolled control passes through one of these.
pub trait A11y: gpui::StatefulInteractiveElement + Sized {
    fn a11y_button(self, label: impl Into<SharedString>) -> Self {
        self.role(gpui::Role::Button).aria_label(label)
    }

    fn a11y_toggle(self, label: impl Into<SharedString>, on: bool) -> Self {
        self.role(gpui::Role::Button).aria_label(label).aria_toggled(if on {
            Toggled::True
        } else {
            Toggled::False
        })
    }

    fn a11y_switch(self, label: impl Into<SharedString>, on: bool) -> Self {
        self.role(gpui::Role::Switch).aria_label(label).aria_toggled(if on {
            Toggled::True
        } else {
            Toggled::False
        })
    }

    fn a11y_menu_item(self, label: impl Into<SharedString>) -> Self {
        self.role(gpui::Role::MenuItem).aria_label(label)
    }

    fn a11y_tab(self, label: impl Into<SharedString>, selected: bool) -> Self {
        self.role(gpui::Role::Tab)
            .aria_label(label)
            .aria_selected(selected)
    }

    fn a11y_option(self, label: impl Into<SharedString>, selected: bool) -> Self {
        self.role(gpui::Role::ListBoxOption)
            .aria_label(label)
            .aria_selected(selected)
    }

    /// `expanded` only attaches when the item can collapse (group headers,
    /// folder rows); plain leaves pass `None`.
    fn a11y_tree_item(self, label: impl Into<SharedString>, expanded: Option<bool>) -> Self {
        let item = self.role(gpui::Role::TreeItem).aria_label(label);
        match expanded {
            Some(expanded) => item.aria_expanded(expanded),
            None => item,
        }
    }
}

impl<T: gpui::StatefulInteractiveElement> A11y for T {}

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

/// Branch labels display at most this many characters before the head is
/// trimmed; the repos branch column owns the budget, and the branch picker
/// falls back to it once the full name outgrows the rail.
pub const BRANCH_LABEL_CHARS: usize = 18;

/// Long branch names shed whole leading path segments —
/// `users/enginkirmaci/test` → `…enginkirmaci/test` — and a name whose last
/// segment alone exceeds the budget sheds characters, so the tail that
/// disambiguates the branch stays visible.
pub fn trim_branch_label(branch: &str, budget: usize) -> String {
    if branch.chars().count() <= budget {
        return branch.to_string();
    }
    let mut tail = branch;
    while let Some((_, rest)) = tail.split_once('/') {
        tail = rest;
        if rest.chars().count() < budget {
            return format!("…{rest}");
        }
    }
    let keep = budget.saturating_sub(1);
    let skipped = tail.chars().count() - keep;
    let mut label: String = tail.chars().skip(skipped).collect();
    label.insert(0, '…');
    label
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
                            .a11y_button("Close drawer")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_branches_pass_through() {
        assert_eq!(trim_branch_label("main", BRANCH_LABEL_CHARS), "main");
        assert_eq!(trim_branch_label("users/enginkirma", BRANCH_LABEL_CHARS), "users/enginkirma");
    }

    #[test]
    fn long_branches_shed_whole_segments() {
        assert_eq!(
            trim_branch_label("users/enginkirmaci/test", BRANCH_LABEL_CHARS),
            "…enginkirmaci/test"
        );
        assert_eq!(
            trim_branch_label("feature/one/two/three/four", BRANCH_LABEL_CHARS),
            "…two/three/four"
        );
    }

    #[test]
    fn oversized_last_segment_sheds_characters() {
        assert_eq!(
            trim_branch_label("a-very-long-branch-name-without-slashes", 10),
            "…t-slashes"
        );
    }
}
