//! ui module: shared helpers + the main window.

pub mod azure_pull;
pub mod azure_tab;
pub mod changes;
pub mod clipboard_password;
pub mod detail;
pub mod dialogs;
pub mod diff_controls;
pub mod file_tree;
pub mod flow_strip;
pub mod formatters;
pub mod issue_detail;
pub mod launchers;
pub mod markdown;
pub mod merge;
pub mod new_pull;
pub mod nuget;
pub mod overview;
pub mod pomodoro;
pub mod pr_issues;
pub mod pull_request;
pub mod readme;
pub mod repos;
pub mod repos_list;
pub mod resize;
pub mod settings;
pub mod summary;
pub mod window;
pub mod windows_tools;
pub mod worktrees;

pub use warpui_kit_core::{A11y, icon};

use gpui::prelude::*;
use gpui::{
    AnyView, Context, FontWeight, IntoElement, Render, SharedString, Stateful, Styled, Window, div,
    px,
};

/// The destructive action: `danger` fill with `on-danger` text, so it never
/// reads as the surface's ordinary primary. `id` is the element id and the
/// probe selector.
pub fn danger_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = id.into();
    let label = label.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .a11y_button(label.clone())
        .cursor_pointer()
        .rounded(px(6.))
        .h(warpui_kit_core::shell::control_height())
        .px(px(12.))
        .flex()
        .items_center()
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .bg(theme::DANGER)
        .text_color(theme::ON_DANGER)
        .hover(|s| s.opacity(0.9))
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

use crate::theme;

// ---- tooltips --------------------------------------------------------------
// The family tooltip paints the shared palette, which this app no longer uses;
// this one draws in the redesign's tokens. Every `tooltip_text` /
// `with_tooltip` call site in the app goes through here.

struct AppTooltip {
    text: SharedString,
}

impl Render for AppTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .rounded(px(4.))
            .border_1()
            .border_color(theme::hairline())
            .bg(theme::popup())
            .px(px(8.))
            .py(px(4.))
            .text_size(px(theme::font_row_meta()))
            .text_color(theme::text_dim())
            .shadow_md()
            .whitespace_nowrap()
            .child(self.text.clone())
    }
}

/// The app's tooltip body builder (grey-on-popup, 9.5px, 4px corners).
pub fn tooltip_text(
    text: impl Into<SharedString>,
) -> impl Fn(&mut Window, &mut gpui::App) -> AnyView + 'static {
    let text = text.into();
    move |_, cx| {
        let text = text.clone();
        cx.new(move |_| AppTooltip { text }).into()
    }
}

/// Attaches a tooltip when the host's "show tooltips" setting is on.
pub fn with_tooltip(
    element: Stateful<gpui::Div>,
    show: bool,
    text: impl Into<SharedString> + 'static,
) -> Stateful<gpui::Div> {
    if show {
        let text = text.into();
        element.tooltip(tooltip_text(text))
    } else {
        element
    }
}

/// The label room inside a branch picker trigger `trigger_w` wide: the 1px
/// border and 8px padding per side, the 8px gap to the chevron, the chevron,
/// the branch icon and the 8px gap after it, less 4px of slack. The icons are
/// read at their live size because the text scale moves them.
pub fn branch_trigger_label_width(trigger_w: f32) -> f32 {
    trigger_w - 38. - 2. * theme::icon_small()
}

/// The label room in one branch picker row of a `panel_w`-wide panel: the 4px
/// panel padding and 8px row padding per side, the check icon and its 4px
/// gap, less 8px of slack for the selected row's semibold face.
pub fn branch_row_label_width(panel_w: f32) -> f32 {
    panel_w - 36. - theme::icon_small()
}

/// The branch picker's label: the full name while its shaped width fits
/// `max_px`, otherwise the longest head-trimmed tail that does. The budget is
/// a width, never a character count, so a row uses the room it actually has.
pub fn branch_label(branch: &str, max_px: f32, window: &gpui::Window) -> String {
    let max = px(max_px.max(0.));
    trim_branch_label(branch, |candidate| shaped_width(candidate, window) <= max)
}

/// The shaped width of `text` at the size the picker draws branch labels in.
fn shaped_width(text: &str, window: &gpui::Window) -> gpui::Pixels {
    let run = gpui::TextRun {
        len: text.len(),
        font: gpui::font(theme::ui_font()),
        color: gpui::Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .layout_line(text, px(theme::font_body()), &[run], None)
        .width
}

/// Characters between the minimal head cut and the next path boundary that a
/// whole-segment cut may still claim: a short leftover segment is worth
/// dropping for a clean edge, a long one is not.
const SLASH_SNAP_CHARS: usize = 4;

/// Long branch names shed their head only as far as the room requires: the
/// longest tail that fits stays, and the cut snaps forward to a path boundary
/// when that costs a few characters or fewer (`users/enginkirmaci/test` →
/// `…enginkirmaci/test`, never `…ne/test`).
fn trim_branch_label(branch: &str, fits: impl Fn(&str) -> bool) -> String {
    if fits(branch) {
        return branch.to_string();
    }
    // a shorter tail only gets easier to fit, so the first cut that fits is
    // the longest tail the room allows and a binary search finds it
    let cuts: Vec<usize> = branch.char_indices().skip(1).map(|(ix, _)| ix).collect();
    let mut lo = 0;
    let mut hi = cuts.len();
    while lo < hi {
        let mid = (lo + hi) / 2;
        if fits(&format!("…{}", &branch[cuts[mid]..])) {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    let Some(&cut) = cuts.get(lo) else {
        return "…".to_string();
    };
    let mut cut = cut;
    // never leave a separator dangling after the ellipsis
    while branch[cut..].starts_with('/') {
        cut += 1;
    }
    // a cut inside a path segment may still snap forward to its boundary when
    // only a few characters are lost: `…two/three` reads better than
    // `…ne/two/three`
    if !branch[..cut].ends_with('/')
        && let Some(slash) = branch[cut..].find('/')
    {
        let after = cut + slash + 1;
        if branch[cut..after].chars().count() <= SLASH_SNAP_CHARS {
            cut = after;
        }
    }
    format!("…{}", &branch[cut..])
}

/// A tool page's title row: its title and a one-line hint.
pub fn tool_header(title: &'static str, hint: &'static str) -> gpui::AnyElement {
    let mut header = div().flex().flex_col().gap_0p5().child(
        div()
            .text_size(px(theme::font_screen_title()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::text())
            .child(title),
    );
    if !hint.is_empty() {
        header = header.child(
            div()
                .text_size(px(theme::font_meta()))
                // the prototype's `.detail-sub` is `t-meta` on `text-2`, not
                // the quietest ink
                .text_color(theme::text_dim())
                .child(hint),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .pb_3()
        .border_b_1()
        .border_color(theme::border_light())
        .child(header)
        .into_any_element()
}

/// [`tool_header`] with the head's own actions (the design gives every tool
/// head its buttons: Copy on Clipboard Password, Register source and the
/// watch toggle on NuGet). The title shrinks and the actions wrap, so a
/// narrow pane squeezes the words instead of pushing the buttons past the
/// window edge.
pub fn tool_header_actions(
    title: &'static str,
    hint: impl IntoElement,
    actions: impl IntoElement,
) -> gpui::AnyElement {
    // the design's detail head: title and actions share one row (the title
    // clips, the actions keep their width) and the hint sits under it
    div()
        .flex()
        .flex_col()
        .gap_1()
        .pb_3()
        .border_b_1()
        .border_color(theme::border_light())
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::font_screen_title()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title),
                )
                .child(actions.into_any_element()),
        )
        .child(
            div()
                .text_size(px(theme::font_meta()))
                // the prototype's `.detail-sub` is `t-meta` on `text-2`, not
                // the quietest ink
                .text_color(theme::text_dim())
                .child(hint.into_any_element()),
        )
        .into_any_element()
}

/// The head's action cluster. Each action is its own child of this row so
/// every one of them reaches the accessibility tree: nesting two buttons in a
/// single `div` child left the second one out of the tree, which made the
/// primary unreachable for a screen reader.
pub fn head_actions(actions: Vec<gpui::AnyElement>) -> gpui::AnyElement {
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .flex_shrink_0()
        .gap_2();
    for (index, action) in actions.into_iter().enumerate() {
        // gpui's accesskit pass only names the first interactive child of a
        // row, so each action goes in its own id-ed wrapper: the wrapper is
        // the row's child and the button inside it is the first of its own
        // parent, which is what the tree walks
        row = row.child(
            div()
                .id(("head-action", index as u64))
                .flex_shrink_0()
                .flex()
                .items_center()
                .child(action),
        );
    }
    row.into_any_element()
}

/// The design's plain button: `panel` fill, `line-strong` edge, one control
/// height. The family's buttons are neutral; only the page's one primary
/// takes the accent fill.
pub fn neutral_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let label = label.into();
    plain_button(id, label.clone(), label, enabled, on_click)
}

/// A neutral button whose accessible name says more than its short visible
/// label — a repeated row action, where "Current" alone is ambiguous.
pub fn neutral_button_named(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    aria: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    plain_button(id, label.into(), aria.into(), enabled, on_click)
}

fn plain_button(
    id: impl Into<SharedString>,
    label: SharedString,
    aria: SharedString,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = id.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .a11y_button(aria)
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::raised()))
                .on_click(on_click)
        })
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1p5()
        .h(warpui_kit_core::shell::control_height())
        .px_3()
        .rounded(px(6.))
        .border_1()
        .border_color(theme::line_strong())
        .bg(theme::panel())
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::text())
        .when(!enabled, |d| d.opacity(0.45))
        .child(label)
        .into_any_element()
}

/// The design's primary button: the one `accent` fill a view carries, with
/// `on-accent` text.
pub fn primary_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = id.into();
    let label = label.into();
    let selector = id.to_string();
    div()
        .id(id)
        .debug_selector(move || selector.clone())
        .a11y_button(label.clone())
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(theme::accent_hover()))
                .on_click(on_click)
        })
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap_1p5()
        .h(warpui_kit_core::shell::control_height())
        .px_3()
        .rounded(px(6.))
        .bg(theme::accent())
        .text_size(px(theme::font_control()))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme::on_accent())
        .when(!enabled, |d| d.opacity(0.45))
        .child(label)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the shaped width: `fits` by character count, so the
    /// trimming rule is testable without a window.
    fn chars_fit(budget: usize) -> impl Fn(&str) -> bool {
        move |text: &str| text.chars().count() <= budget
    }

    #[test]
    fn short_branches_pass_through() {
        assert_eq!(trim_branch_label("main", chars_fit(18)), "main");
        assert_eq!(
            trim_branch_label("users/enginkirma", chars_fit(18)),
            "users/enginkirma"
        );
    }

    #[test]
    fn long_branches_shed_whole_segments() {
        assert_eq!(
            trim_branch_label("users/enginkirmaci/test", chars_fit(18)),
            "…enginkirmaci/test"
        );
        assert_eq!(
            trim_branch_label("feature/one/two/three/four", chars_fit(18)),
            "…two/three/four"
        );
    }

    #[test]
    fn segment_shedding_keeps_the_longest_tail_that_fits() {
        // a fixed character cap used to cut this to `…develop/sonar` even
        // though the longer tail fitted the room
        assert_eq!(
            trim_branch_label("users/enginkirmaci/develop/sonar", chars_fit(30)),
            "…enginkirmaci/develop/sonar"
        );
    }

    #[test]
    fn a_long_segment_keeps_a_partial_tail_instead_of_dropping_it() {
        // the cut lands inside `enginkirmaci` and the boundary is too far to
        // snap to, so the tail stays longer than the old whole-segment rule's
        // `…develop/sonar`
        assert_eq!(
            trim_branch_label("users/enginkirmaci/develop/sonar", chars_fit(22)),
            "…kirmaci/develop/sonar"
        );
    }

    #[test]
    fn oversized_last_segment_sheds_characters() {
        assert_eq!(
            trim_branch_label("a-very-long-branch-name-without-slashes", chars_fit(10)),
            "…t-slashes"
        );
    }
}
