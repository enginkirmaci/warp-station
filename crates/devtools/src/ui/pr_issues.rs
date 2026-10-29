//! The Pull Requests / Issues tabs, in the .NET bar's shape: an "N open …"
//! header with refresh + GitHub shortcuts, then one row per item — the item
//! glyph, bold #number + title, a head → base branch line (PRs) or label
//! chips (issues), and an author / age / Review pill cluster on the right.

use crate::store::Store;
use crate::theme;
use crate::ui::icon;
use crate::ui::A11y;
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
        let title = if self.pull_requests { "pull requests" } else { "issues" };

        let Some(snapshot) = snapshot else {
            return tab_card(empty_state(format!("No GitHub activity loaded — the {title} tab fills after the gh pass settles")));
        };
        if !snapshot.available {
            return tab_card(empty_state("This repository is not on GitHub (or the gh CLI could not authenticate)".into()));
        }

        let items = if self.pull_requests { &snapshot.pulls } else { &snapshot.issues };

        // header: open-item count + refresh and GitHub shortcuts
        let store = self.store.clone();
        let url = snapshot.url.clone();
        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .px_3()
            .py_1()
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(format!("{} open {title}", items.len())),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("pr-refresh")
                    .a11y_button("Refresh panel")
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |s, cx| s.refresh_panel(cx));
                    })
                    .child(icon("icons/icon-refresh.svg", px(theme::icon_small()), theme::TEXT_MUTED)),
            )
            .when_some(url.clone(), |row, url| {
                row.child(
                    div()
                        .id("pr-open-github")
                        .a11y_button("Open on GitHub")
                        .cursor_pointer()
                        .rounded_md()
                        .size(px(26.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, _| {
                            let _ = devtools_core::terminal::open_folder(&url);
                        })
                        .child(icon(
                            "icons/icon-github.svg",
                            px(theme::icon_small()),
                            theme::TEXT_MUTED,
                        )),
                )
            });

        if items.is_empty() {
            return tab_card(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(empty_state(format!("No open {title}"))),
            );
        }

        let mut list = div()
            .id(if self.pull_requests { "pr-list" } else { "issue-list" })
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_2()
            .pt_1();

        for item in items {
            list = list.child(self.render_row(item));
        }
        tab_card(
            div()
                .size_full()
                .flex()
                .flex_col()
                .min_h_0()
                .child(header)
                .child(list),
        )
    }
}

impl ItemListView {
    fn render_row(&mut self, item: &GitHubItem) -> impl IntoElement + use<> {
        let item = item.clone();
        let url = item.url.clone();
        let kind = if item.is_pull_request { "PR" } else { "Issue" };
        let glyph_color = if item.is_pull_request { theme::PURPLE_BRIGHT } else { theme::SUCCESS_BRIGHT };
        let age = item
            .updated_at
            .as_deref()
            .and_then(devtools_core::rel_time::format_rfc3339);

        // title line: bold #number + the title in the bright text color
        let title_line = div()
            .flex()
            .flex_row()
            .items_baseline()
            .gap_1p5()
            .min_w_0()
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::TEXT)
                    .child(format!("#{}", item.number)),
            )
            .child(
                div()
                    .min_w_0()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(item.title.clone()),
            );

        // second line: PRs show head → base; issues show their labels
        let mut second_line = div().flex().flex_row().items_center().gap_1p5().min_w_0();
        if item.is_pull_request {
            let head = item.head_ref_name.clone().unwrap_or_default();
            let base = item.base_ref_name.clone().unwrap_or_default();
            if !head.is_empty() {
                second_line = second_line.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .min_w_0()
                        .text_size(px(theme::font_small() - 1.))
                        .font_family(gpui::SharedString::from(theme::code_font()))
                        .text_color(theme::TEXT_MUTED)
                        .overflow_hidden()
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(head),
                        )
                        .child("→")
                        .child(base),
                );
            }
        }
        for label in &item.labels {
            second_line = second_line.child(
                div()
                    .rounded_md()
                    .bg(theme::tint(theme::BLUE_BRIGHT))
                    .px_1p5()
                    .text_size(px(theme::font_small() - 2.))
                    .text_color(theme::BLUE_BRIGHT)
                    .child(label.clone()),
            );
        }

        // right cluster: author, age, then the decision pill (Review pill in
        // the .NET row; approval states recolor it)
        let mut right = div().flex().flex_row().items_center().gap_2().flex_shrink_0();
        right = right.child(
            div()
                .text_size(px(theme::font_small() - 1.))
                .text_color(theme::TEXT_FAINT)
                .child(item.author.clone().unwrap_or_else(|| "—".into())),
        );
        if let Some(age) = age {
            right = right.child(
                div()
                    .text_size(px(theme::font_small() - 1.))
                    .text_color(theme::TEXT_FAINT)
                    .child(age),
            );
        }
        if item.is_draft {
            right = right.child(
                div()
                    .rounded_md()
                    .bg(theme::CHIP)
                    .px_2()
                    .py_0p5()
                    .text_size(px(theme::font_small() - 1.))
                    .text_color(theme::TEXT_MUTED)
                    .child("draft"),
            );
        } else if item.is_pull_request {
            let (label, color) = match item.review_decision.as_deref() {
                Some("APPROVED") => ("Approved", theme::SUCCESS_BRIGHT),
                Some("CHANGES_REQUESTED") => ("Changes requested", theme::DANGER),
                _ => ("Review", theme::PURPLE_BRIGHT),
            };
            right = right.child(
                div()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme::tint(color))
                    .bg(theme::tint(color))
                    .px_2()
                    .py_0p5()
                    .text_size(px(theme::font_small() - 1.))
                    .text_color(color)
                    .child(label),
            );
        }

        div()
            .id(SharedString::from(format!("{}-{}", kind, item.number)))
            .a11y_button(format!(
                "{} #{} {} — {}",
                kind,
                item.number,
                item.title,
                item.author.clone().unwrap_or_else(|| "—".into())
            ))
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
            .child(icon(
                if item.is_pull_request {
                    "icons/icon-git-pull-request.svg"
                } else {
                    "icons/icon-issue.svg"
                },
                px(theme::icon_small()),
                glyph_color,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(title_line)
                    .when(
                        item.is_pull_request || !item.labels.is_empty(),
                        |block| block.child(second_line),
                    ),
            )
            .child(right)
    }
}

/// The Changes tab's raised-card recipe: the tab's content sits on one
/// rounded dark card floating on the bar surface, inset by the same 8px
/// padding the Changes sections use.
fn tab_card(content: impl IntoElement) -> gpui::AnyElement {
    div()
        .size_full()
        .min_h_0()
        .p_2()
        .flex()
        .flex_col()
        .child(
            div()
                .flex_1()
                .min_h_0()
                .rounded_lg()
                .bg(theme::STRONG_BACKGROUND)
                .overflow_hidden()
                .flex()
                .flex_col()
                .child(content),
        )
        .into_any_element()
}

fn empty_state(text: String) -> gpui::AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_body()))
        .text_color(theme::TEXT_FAINT)
        .child(text)
        .into_any_element()
}
