//! The Overview tab: the repo's README with a details sidebar (GitHub
//! owner/language/license/topics via the gh details probe, plus the local
//! folder).

use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use crate::ui::readme::ReadmeView;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, Window, div, px, relative};

pub struct OverviewView {
    pub store: Entity<Store>,
    readme: Entity<ReadmeView>,
}

impl OverviewView {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let readme = cx.new(|cx| ReadmeView::new(store.clone(), cx));
        Self { store, readme }
    }
}

impl Render for OverviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let details = self.store.read(cx).github_details.clone();
        let entry = self.store.read(cx).selected_entry().cloned();

        // 60/40: README column + details sidebar — both as raised cards on
        // the bar surface, the Changes tab's section recipe
        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .gap_2()
            .p_2()
            .debug_selector(|| "overview-root".into())
            .child(
                div()
                    .id("readme-column")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .rounded_lg()
                    .bg(theme::STRONG_BACKGROUND)
                    .overflow_hidden()
                    .debug_selector(|| "readme-column".into())
                    .child(self.readme.clone()),
            )
            .child(render_sidebar(
                folder,
                entry,
                details,
                self.store.clone(),
                cx,
            ))
    }
}

fn render_sidebar(
    folder: Option<String>,
    entry: Option<devtools_core::repo_store::RepoEntry>,
    details: Option<devtools_core::github::GitHubRepoDetails>,
    store: Entity<Store>,
    cx: &Context<OverviewView>,
) -> gpui::AnyElement {
    let settings = store.read(cx).settings();
    // three stat tiles: icon + count over the label, each opens its tab
    let tile = |id: &'static str,
                label: &'static str,
                count: u32,
                color: gpui::Rgba,
                glyph: &'static str,
                tab: BottomBarTab| {
        let store = store.clone();
        div()
            .id(id)
            .a11y_button(format!("{count} {label}"))
            .cursor_pointer()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(theme::HAIRLINE)
            // CARD on the STRONG card reads as a raised tile (the Changes
            // sections' chip recipe); STRONG would melt into its own card
            .bg(theme::CARD)
            .p_2()
            .debug_selector(move || id.to_string())
            .hover(|s| s.border_color(theme::TEXT_FAINT))
            .on_click(move |_, _, cx| {
                let folder = store.read(cx).selected_repo.clone().unwrap_or_default();
                store.update(cx, |s, cx| s.open_repo_tab(&folder, tab, cx));
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .text_size(px(theme::font_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(icon(glyph, px(theme::icon_small()), color))
                    .child(format!("{count}")),
            )
            .child(
                div()
                    .min_w_0()
                    // never break a word: at the tightest tile widths the
                    // label ellipsizes instead of splitting ("Change s")
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(theme::font_small() - 1.))
                    .text_color(theme::TEXT_MUTED)
                    .child(label),
            )
    };
    let modified = entry.as_ref().map(|e| e.git_modified_count).unwrap_or(0);
    let prs = entry.as_ref().map(|e| e.github_pr_count).unwrap_or(0);
    let issues = entry.as_ref().map(|e| e.github_issue_count).unwrap_or(0);

    let mut side = div()
        .id("overview-sidebar")
        .w(relative(0.4))
        // the floor grows with the text so the tile labels keep fitting when
        // the system font scale is large (the tiles' room comes from here)
        .min_w(px(280.) * (theme::font_small() / 12.0))
        .h_full()
        .flex()
        .flex_col()
        .gap_2()
        .p_4()
        .rounded_lg()
        .bg(theme::STRONG_BACKGROUND)
        .overflow_y_scroll()
        .debug_selector(|| "overview-sidebar".into())
        .child(
            div()
                .flex()
                .flex_row()
                .gap_1p5()
                .debug_selector(|| "tiles-row".into())
                .child(tile(
                    "tile-changes",
                    "Changes",
                    modified,
                    theme::AMBER_BRIGHT,
                    "icons/icon-code.svg",
                    BottomBarTab::Changes,
                ))
                .when(settings.repos.enable_git_hub, |row| {
                    row.child(tile(
                        "tile-prs",
                        "PRs",
                        prs,
                        theme::PURPLE_BRIGHT,
                        "icons/icon-git-pull-request.svg",
                        BottomBarTab::PullRequests,
                    ))
                    .child(tile(
                        "tile-issues",
                        "Issues",
                        issues,
                        theme::SUCCESS_BRIGHT,
                        "icons/icon-issue.svg",
                        BottomBarTab::Issues,
                    ))
                }),
        )
        .child(
            div()
                .text_size(px(theme::font_title()))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Repository Details"),
        );
    if let Some(folder) = &folder {
        side = side.child(link_detail_row("Folder", folder.clone()));
    }
    if let Some(entry) = &entry {
        if let Some(branch) = entry.git_branch_name.as_deref().filter(|b| !b.is_empty()) {
            side = side.child(detail_row("Branch", branch.to_string()));
        }
        let sync = if entry.git_to_push_count == 0 && entry.git_to_pull_count == 0 {
            "Up to date".to_string()
        } else {
            format!(
                "{} to push · {} to pull",
                entry.git_to_push_count, entry.git_to_pull_count
            )
        };
        side = side.child(detail_row("Sync", sync));
        if let Some(at) = entry.git_last_commit_at {
            side = side.child(detail_row(
                "Last commit",
                devtools_core::rel_time::format(at),
            ));
        }
    }
    if let Some(details) = &details {
        if let Some(url) = &details.url {
            side = side.child(link_detail_row("GitHub", url.clone()));
        }
        if let Some(owner) = &details.owner {
            side = side.child(detail_row("Owner", owner.clone()));
        }
        if let Some(created) = &details.created_at {
            side = side.child(detail_row(
                "Created",
                crate::ui::inspector::format_created(created),
            ));
        }
        if let Some(language) = &details.primary_language {
            side = side.child(detail_row("Language", language.clone()));
        }
        if let Some(license) = &details.license {
            side = side.child(detail_row("License", license.clone()));
        }
        if let Some(branch) = &details.default_branch {
            side = side.child(detail_row("Default Branch", branch.clone()));
        }
        if !details.topics.is_empty() {
            side = side.child(
                div()
                    .flex()
                    .flex_row()
                    .items_start()
                    .child(
                        div()
                            .w(px(110.))
                            .flex_shrink_0()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::TEXT_MUTED)
                            .child("Topics"),
                    )
                    .child(div().flex().flex_row().flex_wrap().gap_1().children(
                        details.topics.iter().map(|topic| {
                            div()
                                .rounded_md()
                                .bg(theme::tint(theme::BLUE_BRIGHT))
                                .px_1p5()
                                .text_size(px(theme::font_small() - 2.))
                                .text_color(theme::BLUE_BRIGHT)
                                .child(topic.clone())
                        }),
                    )),
            );
        }
    }
    side.into_any_element()
}

/// The .NET details card: muted label column on the left, value on the right.
fn detail_row(label: &str, value: String) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_row()
        .items_start()
        .child(
            div()
                .w(px(110.))
                .flex_shrink_0()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_MUTED)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_DIM)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(value),
        )
}

/// A detail row whose value opens in the system opener, with the .NET row's
/// trailing external-link glyph.
fn link_detail_row(label: &str, value: String) -> impl IntoElement + use<> {
    let open_value = value.clone();
    div()
        .id(gpui::SharedString::from(format!("detail-link-{label}")))
        .a11y_button(format!("Open {label}"))
        .cursor_pointer()
        .flex()
        .flex_row()
        .items_start()
        .hover(|s| s.bg(theme::HOVER))
        .on_click(move |_, _, _| {
            let _ = devtools_core::terminal::open_folder(&open_value);
        })
        .child(
            div()
                .w(px(110.))
                .flex_shrink_0()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_MUTED)
                .child(label.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .overflow_hidden()
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_DIM)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(value),
                )
                .child(icon(
                    "icons/icon-open-in-new.svg",
                    px(theme::icon_small() - 2.),
                    theme::TEXT_FAINT,
                )),
        )
}
