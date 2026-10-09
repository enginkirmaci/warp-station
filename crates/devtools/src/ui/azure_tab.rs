//! The Pipelines (Azure DevOps) tab: the snapshot's pipeline runs with
//! status glyphs, plus the project's active pull requests and open work
//! items from the same REST pass.

use crate::store::Store;
use crate::theme;
use devtools_core::azure::AzureItem;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};

pub struct AzureView {
    pub store: Entity<Store>,
}

impl Render for AzureView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let snapshot = folder.as_ref().and_then(|f| self.store.read(cx).azure_by_folder.get(f).cloned());

        let Some(snapshot) = snapshot else {
            return empty_state("No Azure DevOps activity loaded — the pass fills after the scan settles".into());
        };
        if !snapshot.available {
            return empty_state("This repository has no Azure DevOps remote (or the PAT was rejected)".into());
        }

        let mut list = div()
            .id("azure-list")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_2()
            .gap_2();

        if !snapshot.pipelines.is_empty() {
            list = list.child(section("Pipelines"));
            for run in &snapshot.pipelines {
                let (glyph, color, label) = if run.is_running() {
                    ("◐", theme::BLUE_BRIGHT, "running")
                } else if run.is_failed() {
                    ("✗", theme::DANGER, "failed")
                } else if run.is_succeeded() {
                    ("✓", theme::SUCCESS_BRIGHT, "succeeded")
                } else {
                    ("·", theme::TEXT_MUTED, "queued")
                };
                let url = run.url.clone().unwrap_or_default();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("pipeline-{}", run.id)))
                        .cursor_pointer()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .rounded_md()
                        .px_2()
                        .py_1p5()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, _| {
                            if !url.is_empty() {
                                let _ = devtools_core::terminal::open_folder(&url);
                            }
                        })
                        .child(div().text_size(px(theme::FONT_BODY)).text_color(color).child(glyph))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .text_size(px(theme::FONT_SMALL))
                                        .text_color(theme::TEXT_DIM)
                                        .child(run.build_number.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(theme::FONT_SMALL - 1.))
                                        .text_color(theme::TEXT_FAINT)
                                        .child(run.definition_name.clone().unwrap_or_default()),
                                ),
                        )
                        .when_some(run.display_branch(), |row, branch| {
                            row.child(
                                div()
                                    .rounded_md()
                                    .bg(theme::tint(theme::PURPLE_BRIGHT))
                                    .px_1p5()
                                    .text_size(px(theme::FONT_SMALL - 2.))
                                    .text_color(theme::PURPLE_BRIGHT)
                                    .child(branch),
                            )
                        })
                        .child(
                            div()
                                .text_size(px(theme::FONT_SMALL - 1.))
                                .text_color(color)
                                .child(label),
                        ),
                );
            }
        }

        if !snapshot.pulls.is_empty() {
            list = list.child(section("Pull requests"));
            for item in &snapshot.pulls {
                list = list.child(item_row(item, "PR"));
            }
        }
        if !snapshot.work_items.is_empty() {
            list = list.child(section("Work items"));
            for item in &snapshot.work_items {
                list = list.child(item_row(item, "WI"));
            }
        }

        if snapshot.pipelines.is_empty() && snapshot.pulls.is_empty() && snapshot.work_items.is_empty() {
            return empty_state("No activity on Azure DevOps".into());
        }
        list.into_any_element()
    }
}

fn section(label: &str) -> impl IntoElement + use<> {
    div()
        .px_1()
        .py_1()
        .text_size(px(theme::FONT_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme::TEXT_MUTED)
        .child(label.to_string())
}

fn item_row(item: &AzureItem, kind: &str) -> impl IntoElement + use<> {
    let item = item.clone();
    let url = item.url.clone();
    let kind_owned = kind.to_string();
    div()
        .id(SharedString::from(format!("azure-{kind_owned}-{}", item.number)))
        .cursor_pointer()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .rounded_md()
        .px_2()
        .py_1p5()
        .hover(|s| s.bg(theme::HOVER))
        .on_click(move |_, _, _| {
            if !url.is_empty() {
                let _ = devtools_core::terminal::open_folder(&url);
            }
        })
        .child(
            div()
                .rounded_sm()
                .bg(theme::tint(theme::BLUE_BRIGHT))
                .text_color(theme::BLUE_BRIGHT)
                .text_size(px(theme::FONT_SMALL - 1.))
                .px_1()
                .child(format!("{kind_owned} #{}", item.number)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::FONT_SMALL))
                .text_color(theme::TEXT_DIM)
                .child(item.title.clone()),
        )
        .when_some(item.state.clone(), |row, state| {
            row.child(
                div()
                    .text_size(px(theme::FONT_SMALL - 1.))
                    .text_color(theme::TEXT_FAINT)
                    .child(state),
            )
        })
        .child(
            div()
                .text_size(px(theme::FONT_SMALL - 1.))
                .text_color(theme::TEXT_FAINT)
                .child(item.author.clone().unwrap_or_default()),
        )
}

fn empty_state(text: String) -> gpui::AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::FONT_BODY))
        .text_color(theme::TEXT_FAINT)
        .child(text)
        .into_any_element()
}
