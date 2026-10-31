//! ui module: shared helpers + the main window.

pub mod azure_tab;
pub mod bottom_bar;
pub mod changes;
pub mod clipboard_password;
pub mod dialogs;
pub mod formatters;
pub mod nuget;
pub mod overview;
pub mod pr_issues;
pub mod repos;
pub mod repos_proto;
pub mod settings;
pub mod window;

pub use warpui_kit_core::tooltip::{tooltip_text, with_tooltip};
pub use warpui_kit_core::{A11y, icon};

use gpui::prelude::*;
use gpui::{FontWeight, Styled, div, px};

use crate::theme;

/// Branch labels display at most this many characters before the head is
/// trimmed; the repos branch column owns the budget, and the branch picker
/// falls back to it once the full name outgrows the rail.
pub const BRANCH_LABEL_CHARS: usize = 18;

/// Width the rail leaves under the trigger's label; the pill's tighter
/// budget exists to cap the table column and does not apply here.
const LABEL_MAX_PX: f32 = 190.;

/// The branch picker's label: the full name while its shaped width fits the
/// rail, the head-trimmed pill label once it would overflow.
pub fn branch_label(branch: &str, window: &gpui::Window) -> String {
    let run = gpui::TextRun {
        len: branch.len(),
        font: gpui::font(theme::ui_font()),
        color: gpui::Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let width = window
        .text_system()
        .layout_line(branch, px(theme::font_small()), &[run], None)
        .width;
    if width <= px(LABEL_MAX_PX) {
        branch.to_string()
    } else {
        trim_branch_label(branch, BRANCH_LABEL_CHARS)
    }
}

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
    let mut header = div().flex().flex_col().gap_0p5().child(
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
        assert_eq!(
            trim_branch_label("users/enginkirma", BRANCH_LABEL_CHARS),
            "users/enginkirma"
        );
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
