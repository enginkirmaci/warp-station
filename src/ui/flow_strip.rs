//! The Changes flow strip: the branch's `Create PR → Merge → Pipeline →
//! Result` path in one row. It is a state, not a wizard: a completed step is a
//! link back, the current one is the next press, and a blocked one is dim.
//!
//! The store owns the derivation (`Store::flow_view`) and every action
//! (`Store::run_flow_action`); this module only draws the four chips.

use crate::store::{FlowStepState, FlowStepView, Store};
use crate::theme;
use crate::ui::{A11y, icon};
use gpui::prelude::*;
use gpui::{AnyElement, Entity, FontWeight, SharedString, div, px, relative};
use warpui_kit_core::shell;

/// The strip is four labelled chips; the detail line only fits on a wide pane.
const COMPACT_BELOW: f32 = 900.;

/// The strip for the selected branch, or nothing when the branch has no flow
/// to show. `cx` is the render's app context: the chips' actions update the
/// store directly.
pub fn render(store: &Entity<Store>, cx: &gpui::App) -> Option<AnyElement> {
    let view = store.read(cx).flow_view()?;
    let compact = store.read(cx).pane_width < COMPACT_BELOW;
    // the strip is the detail pane's first band, above the repo name: it owns
    // its padding and its recessed fill, and its height comes from the chips,
    // which no longer clip their two text lines
    let mut row = div()
        .id("flow-strip")
        .debug_selector(|| "flow-strip".into())
        .flex_shrink_0()
        .w_full()
        .px(px(shell::DETAIL_PADDING))
        .py(px(if compact { 6. } else { 8. }))
        .bg(theme::well())
        .border_b_1()
        .border_color(theme::hairline())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(theme::font_meta()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme::text_faint())
                .child("Flow"),
        );
    for (index, step) in view.steps.iter().enumerate() {
        if index > 0 {
            row = row.child(
                div()
                    .flex_shrink_0()
                    .w(px(16.))
                    .h(px(1.))
                    .bg(theme::hairline()),
            );
        }
        row = row.child(step_chip(index, step, store, compact));
    }
    Some(row.into_any_element())
}

fn step_chip(
    index: usize,
    step: &FlowStepView,
    store: &Entity<Store>,
    compact: bool,
) -> AnyElement {
    let action = step.action.clone();
    let clickable = action.is_some();
    let target = store.clone();
    let current = step.state == FlowStepState::Current;
    let done = step.state == FlowStepState::Done;
    let badge = if done {
        icon(
            "icons/icon-check.svg",
            px(theme::icon_tiny()),
            theme::SUCCESS_BRIGHT,
        )
        .into_any_element()
    } else {
        div()
            .text_size(px(theme::font_meta()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(if current {
                theme::on_accent()
            } else {
                theme::text_dim()
            })
            .child(format!("{}", index + 1))
            .into_any_element()
    };
    let mut chip = div()
        .id(SharedString::from(format!("flow-step-{}", step.key)))
        .debug_selector({
            let key = step.key;
            move || format!("flow-step-{key}")
        })
        .a11y_button(format!("{}: {}", step.label, step.detail))
        .flex_shrink_0()
        .h(px(if compact { 34. } else { 46. }))
        .px(px(12.))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.))
        .rounded(px(6.))
        .border_1()
        .border_color(if current {
            theme::accent()
        } else if done {
            theme::line_strong()
        } else {
            theme::hairline()
        })
        .bg(if current {
            theme::accent_soft()
        } else if done {
            theme::panel()
        } else {
            theme::surface()
        })
        .when(!done && !current, |chip| chip.opacity(0.55))
        .when(clickable, |chip| {
            chip.cursor_pointer()
                .hover(|style| style.bg(theme::raised()))
                .on_click(move |_, _, cx| {
                    if let Some(action) = action.clone() {
                        target.update(cx, |store, cx| store.run_flow_action(action, cx));
                    }
                })
        })
        .child(
            div()
                .flex_shrink_0()
                .size(px(if compact { 18. } else { 20. }))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(if current {
                    theme::accent()
                } else {
                    theme::raised()
                })
                .child(badge),
        );
    if !compact {
        chip = chip.child(
            div()
                .min_w_0()
                .max_w(px(190.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .debug_selector({
                            let key = step.key;
                            move || format!("flow-label-{key}")
                        })
                        .line_height(relative(1.25))
                        .text_size(px(theme::font_control()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if current {
                            theme::text()
                        } else {
                            theme::text_dim()
                        })
                        .child(step.label),
                )
                .child(
                    div()
                        .debug_selector({
                            let key = step.key;
                            move || format!("flow-detail-{key}")
                        })
                        .line_height(relative(1.25))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(step.detail.clone()),
                ),
        );
    } else {
        chip = chip.child(
            div()
                .text_size(px(theme::font_meta()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if current {
                    theme::text()
                } else {
                    theme::text_dim()
                })
                .child(step.label),
        );
    }
    chip.into_any_element()
}
