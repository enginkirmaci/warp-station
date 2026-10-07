//! The Pull Requests / Issues tabs: the open-item lists from the GitHub
//! activity pass, one row per item — number, title, draft/review chips,
//! labels, author — clicking opens the item in the browser.

use crate::store::Store;
use crate::theme;
use devtools_core::github::GitHubItem;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};

pub struct ItemListView {
    pub store: Entity<Store>,
    pub pull_requests: bool,
}

impl Render for ItemListView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let snapshot = folder.as_ref().and_then(|f| self.store.read(cx).github_by_folder.get(f).cloned());
        let title = if self.pull_requests { "Pull Requests" } else { "Issues" };

        let Some(snapshot) = snapshot else {
            return empty_state(format!("No GitHub activity loaded — {title} tab fills after the gh pass settles"));
        };
        if !snapshot.available {
            return empty_state("This repository is not on GitHub (or the gh CLI could not authenticate)".into());
        }

        let items = if self.pull_requests { &snapshot.pulls } else { &snapshot.issues };
        if items.is_empty() {
            return empty_state(format!("No open {title}"));
        }

        let mut list = div()
            .id(if self.pull_requests { "pr-list" } else { "issue-list" })
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_2()
            .gap_0p5();

        for item in items {
            list = list.child(self.render_row(item));
        }
        list.into_any_element()
    }
}

impl ItemListView {
    fn render_row(&mut self, item: &GitHubItem) -> impl IntoElement + use<> {
        let item = item.clone();
        let url = item.url.clone();
        let kind = if item.is_pull_request { "PR" } else { "Issue" };
        let mut labels = div().flex().flex_row().gap_1();
        for label in &item.labels {
            labels = labels.child(
                div()
                    .rounded_md()
                    .bg(theme::tint(theme::BLUE_BRIGHT))
                    .px_1p5()
                    .text_size(px(theme::FONT_SMALL - 2.))
                    .text_color(theme::BLUE_BRIGHT)
                    .child(label.clone()),
            );
        }

        let mut title_row = div().flex().flex_row().items_center().gap_2().min_w_0();
        title_row = title_row
            .child(
                div()
                    .rounded_sm()
                    .bg(theme::tint(if item.is_pull_request {
                        theme::PURPLE_BRIGHT
                    } else {
                        theme::BLUE_BRIGHT
                    }))
                    .text_color(if item.is_pull_request {
                        theme::PURPLE_BRIGHT
                    } else {
                        theme::BLUE_BRIGHT
                    })
                    .text_size(px(theme::FONT_SMALL - 1.))
                    .px_1()
                    .child(format!("{kind} #{}", item.number)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(theme::FONT_SMALL))
                    .text_color(theme::TEXT_DIM)
                    .child(item.title.clone()),
            );
        if item.is_draft {
            title_row = title_row.child(
                div()
                    .rounded_md()
                    .bg(theme::CHIP)
                    .px_1p5()
                    .text_size(px(theme::FONT_SMALL - 2.))
                    .text_color(theme::TEXT_MUTED)
                    .child("draft"),
            );
        }
        if let Some(decision) = item.review_decision.clone().filter(|d| !d.is_empty()) {
            let (label, color) = match decision.as_str() {
                "APPROVED" => ("approved".to_string(), theme::SUCCESS_BRIGHT),
                "CHANGES_REQUESTED" => ("changes requested".to_string(), theme::DANGER),
                _ => (decision.clone(), theme::AMBER_BRIGHT),
            };
            title_row = title_row.child(
                div()
                    .rounded_md()
                    .bg(theme::tint(color))
                    .px_1p5()
                    .text_size(px(theme::FONT_SMALL - 2.))
                    .text_color(color)
                    .child(label),
            );
        }

        div()
            .id(SharedString::from(format!("{}-{}", kind, item.number)))
            .cursor_pointer()
            .flex()
            .flex_col()
            .gap_1()
            .rounded_md()
            .px_2()
            .py_1p5()
            .hover(|s| s.bg(theme::HOVER))
            .on_click(move |_, _, _| {
                if !url.is_empty() {
                    let _ = devtools_core::terminal::open_folder(&url);
                }
            })
            .child(title_row)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(theme::FONT_SMALL - 1.))
                            .text_color(theme::TEXT_FAINT)
                            .child(item.author.clone().unwrap_or_else(|| "—".into())),
                    )
                    .child(labels)
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(theme::FONT_SMALL - 1.))
                            .text_color(theme::TEXT_FAINT)
                            .font_weight(FontWeight::BOLD)
                            .child("open ↗"),
                    ),
            )
    }
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
