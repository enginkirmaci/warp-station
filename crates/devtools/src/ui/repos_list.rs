//! The repos list, the split screen's left pane. Rows are two lines, identity
//! over context, with no column chrome, and colour on a row only means state:
//! the favorite star, work waiting, a pipeline's result. The launchers and the
//! repo verbs live in the rail beside it; folder bands fold their group away.

use crate::store::Store;
use crate::store::projection::ProjectedRow;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use devtools_core::azure::AzureSnapshot;
use devtools_core::rel_time;
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::{Entity, FontWeight, Rgba, SharedString, div, px};

/// The hover group a band's folder glyph swaps its filter mark under.
const BAND_GROUP: &str = "repo-band";

/// What a row shows beyond the entry itself.
struct RowFacts<'a> {
    /// Whether the GitHub integration is on; its counts leave the row if not.
    github: bool,
    azure: Option<&'a AzureSnapshot>,
    /// The workspaces warp-agent is running in this repo, when it has any.
    warp_titles: Option<String>,
    show_tooltips: bool,
}

/// The whole list: one quiet header per folder group, then its rows. Rows
/// select on click — the rail beside them is useless otherwise.
pub fn render(store: &Store, store_entity: &Entity<Store>) -> gpui::AnyElement {
    if store.projected.is_empty() {
        return empty_list(store);
    }
    let mut list = div()
        .id("repos-list")
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .pt_1();

    let settings = store.settings();
    let soloed = store.group_filter.as_deref();

    for (row_ix, row) in store.projected.iter().enumerate() {
        match row {
            ProjectedRow::Group {
                key,
                name,
                count,
                collapsed,
            } => {
                let solo = soloed == Some(key.as_str());
                list = list.child(group_band(
                    key,
                    name,
                    *count,
                    *collapsed,
                    solo,
                    store_entity,
                ));
            }
            ProjectedRow::Repo(ix) => {
                if let Some(repo) = store.repos.get(*ix) {
                    let folder = repo.folder_path.to_string_lossy();
                    let selected = store.selected_repo.as_deref() == Some(folder.as_ref());
                    let azure = store
                        .azure_by_folder
                        .get(folder.as_ref())
                        .filter(|_| settings.repos.enable_azure_dev_ops)
                        .map(|snapshot| snapshot.as_ref());
                    let warp_titles = store
                        .warp_running
                        .get(folder.as_ref())
                        .filter(|status| !status.running.is_empty())
                        .map(|status| status.running.join(", "));
                    let facts = RowFacts {
                        github: settings.repos.enable_git_hub,
                        azure,
                        warp_titles,
                        show_tooltips: settings.general.show_tooltips,
                    };
                    list = list.child(repo_row(repo, row_ix, selected, &facts, store_entity));
                }
            }
        }
    }

    list.into_any_element()
}

/// Why the list has no rows.
fn empty_list(store: &Store) -> gpui::AnyElement {
    let filtered =
        !store.tag_filters.is_empty() || store.group_filter.is_some() || store.attention_only;
    let has_folders = store
        .settings()
        .repos
        .repo_scan_folders
        .is_some_and(|folders| !folders.is_empty());
    let text = if store.scanning {
        "Scanning folders…"
    } else if filtered {
        "No repositories match the current filter"
    } else if has_folders {
        "No repositories found under the scan folders"
    } else {
        "No scan folders yet — add a repository to start"
    };
    div()
        .debug_selector(|| "repos-empty".into())
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_meta()))
        .text_color(theme::TEXT_MUTED)
        .child(text)
        .into_any_element()
}

/// A folder band: the export's 30px row on its own step of the ramp. Pressing
/// it folds the group; its folder glyph narrows the list to this group, and
/// turns into the filter mark on hover and for as long as it is soloed.
fn group_band(
    key: &str,
    name: &str,
    count: usize,
    collapsed: bool,
    soloed: bool,
    store: &Entity<Store>,
) -> gpui::AnyElement {
    let band_selector = format!("repo-band-{key}");
    let solo_selector = format!("repo-band-solo-{key}");
    let fold_target = store.clone();
    let fold_key = key.to_string();
    let solo_target = store.clone();
    let solo_key = key.to_string();

    let solo = div()
        .id(SharedString::from(format!("repo-band-solo-id-{key}")))
        .debug_selector(move || solo_selector.clone())
        .a11y_toggle("Show only this group", soloed)
        .relative()
        .cursor_pointer()
        .flex_shrink_0()
        .size(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(3.))
        .hover(|d| d.bg(theme::BORDER_LIGHT))
        .on_click(move |_, _, cx| {
            // the band's own press folds the group
            cx.stop_propagation();
            solo_target.update(cx, |s, cx| s.solo_group(&solo_key, cx));
        })
        .child(
            icon("icons/icon-folder.svg", px(13.), theme::MUTED_ICON)
                .when(soloed, |glyph| glyph.opacity(0.))
                .group_hover(BAND_GROUP, |s| s.opacity(0.)),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    icon(
                        "icons/icon-filter.svg",
                        px(12.),
                        if soloed {
                            theme::BLUE_BRIGHT
                        } else {
                            theme::MUTED_ICON
                        },
                    )
                    .when(!soloed, |glyph| glyph.opacity(0.))
                    .group_hover(BAND_GROUP, |s| s.opacity(1.)),
                ),
        );

    div()
        .id(SharedString::from(format!("repo-band-id-{key}")))
        .debug_selector(move || band_selector.clone())
        .group(BAND_GROUP)
        .a11y_button(format!("{name} — {count} repositories"))
        .aria_expanded(!collapsed)
        .cursor_pointer()
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(30.))
        .px(px(20.))
        .border_b_1()
        .border_color(theme::BORDER_LIGHT)
        .bg(theme::GROUP_BG)
        .hover(|d| d.bg(theme::HOVER_BG))
        .on_click(move |_, _, cx| {
            fold_target.update(cx, |s, cx| s.toggle_group_collapse(&fold_key, cx));
        })
        .child(icon(
            if collapsed {
                "icons/icon-chevron-right.svg"
            } else {
                "icons/icon-chevron-down.svg"
            },
            px(13.),
            theme::MUTED_ICON,
        ))
        .child(solo)
        .child(
            div()
                .text_size(px(theme::font_sub()))
                .text_color(theme::TEXT_DIM)
                .child(name.to_lowercase()),
        )
        .child(
            div()
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::TEXT_MUTED)
                .child(format!("{count}")),
        )
        .into_any_element()
}

/// One repository: identity on the first line, context on the second, the
/// activity stamp on the right and the open hint only while hovered.
fn repo_row(
    repo: &RepoEntry,
    row_ix: usize,
    selected: bool,
    facts: &RowFacts,
    store: &Entity<Store>,
) -> gpui::AnyElement {
    let favorite = repo.is_favorite();
    let archived = repo.is_archived();
    let name_color = if archived {
        theme::TEXT_MUTED
    } else {
        theme::TEXT
    };
    let folder = repo.folder_path.to_string_lossy().to_string();
    let select_target = store.clone();
    let select_folder = folder.clone();
    let star_target = store.clone();

    // archived rows carry no star: Restore in the rail's menu comes first
    let star = div()
        .id(SharedString::from(format!("repo-star-id-{row_ix}")))
        .debug_selector(move || format!("repo-star-{row_ix}"))
        .a11y_toggle(
            if favorite {
                "Remove from favorites"
            } else {
                "Add to favorites"
            },
            favorite,
        )
        .flex_shrink_0()
        .size(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .when(!archived, |star| {
            star.cursor_pointer()
                .on_click(move |_, _, cx| {
                    // the star is not a press on the row
                    cx.stop_propagation();
                    star_target.update(cx, |s, cx| s.toggle_favorite(&folder, cx));
                })
                .when(favorite, |star| {
                    star.child(icon("icons/icon-star-filled.svg", px(13.), theme::STAR_ON))
                })
                .when(!favorite, |star| {
                    star.opacity(0.62)
                        .group_hover("repo-row", |s| s.opacity(1.))
                        .child(icon("icons/icon-star.svg", px(13.), theme::STAR_MUTED))
                })
        });

    let branch = repo.git_branch_name.clone().unwrap_or_else(|| "—".into());
    let mut meta = div()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(theme::font_row_meta()))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(icon(
                    "icons/icon-source-branch.svg",
                    px(11.),
                    theme::TEXT_MUTED,
                ))
                .child(
                    div()
                        .font_family(theme::code_font())
                        .text_color(theme::TEXT_DIM)
                        .child(SharedString::from(branch)),
                ),
        );
    let (issues, pulls) = if facts.github {
        (repo.github_issue_count, repo.github_pr_count)
    } else {
        (0, 0)
    };
    if repo.git_modified_count > 0 {
        meta = meta.child(counted(
            "icons/icon-file-diff.svg",
            repo.git_modified_count,
            "change",
            theme::AMBER_BRIGHT,
        ));
    }
    if issues > 0 {
        meta = meta.child(counted(
            "icons/icon-circle-dot.svg",
            issues,
            "issue",
            theme::TEXT_DIM,
        ));
    }
    if pulls > 0 {
        meta = meta.child(
            div()
                .debug_selector(move || format!("repo-prs-{row_ix}"))
                .child(counted(
                    "icons/icon-git-pull-request.svg",
                    pulls,
                    "PR",
                    theme::TEXT_DIM,
                )),
        );
    }
    if repo.git_to_push_count > 0 {
        meta = meta.child(
            div()
                .text_color(theme::AMBER_BRIGHT)
                .child(format!("↑{}", repo.git_to_push_count)),
        );
    }
    if repo.git_to_pull_count > 0 {
        meta = meta.child(
            div()
                .debug_selector(move || format!("repo-behind-{row_ix}"))
                .text_color(theme::BLUE_BRIGHT)
                .child(format!("↓{}", repo.git_to_pull_count)),
        );
    }
    if let Some(chips) = facts
        .azure
        .and_then(|snapshot| azure_chips(snapshot, row_ix))
    {
        meta = meta.child(chips);
    }
    if repo.git_modified_count == 0
        && repo.git_to_push_count == 0
        && repo.git_to_pull_count == 0
        && issues == 0
        && pulls == 0
    {
        meta = meta.child(
            div()
                .debug_selector(move || format!("repo-clean-{row_ix}"))
                .flex()
                .items_center()
                .gap(px(4.))
                .text_color(theme::SUCCESS_BRIGHT)
                .child(icon("icons/icon-check.svg", px(11.), theme::SUCCESS_BRIGHT))
                .child("clean"),
        );
    }

    let name_line = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .min_w_0()
        .child(
            div()
                .min_w_0()
                .text_size(px(theme::font_repo_name()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(name_color)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(repo.name.clone()),
        )
        .when_some(facts.warp_titles.clone(), |line, titles| {
            line.child(crate::ui::with_tooltip(
                div()
                    .id(SharedString::from(format!("repo-warp-id-{row_ix}")))
                    .debug_selector(move || format!("repo-warp-{row_ix}"))
                    .flex_shrink_0()
                    .rounded_full()
                    .size(px(7.))
                    .bg(theme::SUCCESS_BRIGHT),
                facts.show_tooltips,
                SharedString::from(format!("warp-agent is running: {titles}")),
            ))
        });

    let identity = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(name_line)
        .child(meta);

    // the mockup's stamp is compact ("12m", "1d"); rel_time says "… ago"
    let stamp = repo
        .git_last_commit_at
        .map(|at| rel_time::format(at).trim_end_matches(" ago").to_string())
        .unwrap_or_else(|| "—".into());

    let row = div()
        .id(SharedString::from(format!("repo-row-id-{row_ix}")))
        .debug_selector(move || format!("repo-row-{row_ix}"))
        .group("repo-row")
        .a11y_button(repo.name.clone())
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            // the rail stays open, so a second press has nothing to toggle
            select_target.update(cx, |s, cx| s.select_repo(&select_folder, cx));
        })
        .flex()
        .items_center()
        .gap(px(8.))
        .h(px(50.))
        .pl(px(18.))
        .pr(px(16.))
        .border_b_1()
        .border_color(theme::BORDER_LIGHT)
        .when(archived, |row| row.opacity(0.55))
        .when(selected, |row| {
            row.bg(theme::SELECTED).text_color(theme::TEXT)
        })
        .when(!selected, |row| row.hover(|s| s.bg(theme::HOVER_BG)))
        .child(star)
        .child(identity)
        .child(
            div()
                .flex_shrink_0()
                .font_family(theme::code_font())
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::TEXT_MUTED)
                .group_hover("repo-row", |s| s.opacity(0.))
                .child(stamp),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(18.))
                .opacity(0.)
                .group_hover("repo-row", |s| s.opacity(1.))
                .child(keycap("↵")),
        );

    // the design marks the selection with a 2px accent edge; a wrapper owns it
    // so the row's own bottom border keeps its colour
    div()
        .flex_shrink_0()
        .border_l_2()
        .border_color(if selected {
            theme::BLUE_BRIGHT
        } else {
            theme::BACKGROUND
        })
        .child(row)
        .into_any_element()
}

/// `21 changes` with the number tinted and the noun quiet.
fn counted(
    icon_path: &'static str,
    count: u32,
    noun: &'static str,
    tint: Rgba,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(4.))
        .child(icon(icon_path, px(11.), tint))
        .child(div().text_color(tint).child(format!("{count}")))
        .child(div().text_color(tint).child(if count == 1 {
            noun.to_string()
        } else {
            format!("{noun}s")
        }))
        .into_any_element()
}

/// The repo's Azure DevOps state: the newest pipeline run's glyph, then the
/// open pull requests and work items. Nothing for a project with no activity.
fn azure_chips(snapshot: &AzureSnapshot, row_ix: usize) -> Option<gpui::AnyElement> {
    if snapshot.pipelines.is_empty() && snapshot.pulls.is_empty() && snapshot.work_items.is_empty()
    {
        return None;
    }
    let mut chips = div()
        .debug_selector(move || format!("repo-azure-{row_ix}"))
        .flex()
        .items_center()
        .gap(px(4.))
        .text_color(theme::TEXT_DIM);
    if !snapshot.pipelines.is_empty() {
        let (glyph, color) = if snapshot.pipeline_running() {
            ("icons/icon-play.svg", theme::BLUE_BRIGHT)
        } else if snapshot.pipeline_failed() {
            ("icons/icon-close.svg", theme::DANGER)
        } else if snapshot.pipeline_ok() {
            ("icons/icon-check.svg", theme::SUCCESS_BRIGHT)
        } else {
            ("icons/icon-clock.svg", theme::TEXT_MUTED)
        };
        chips = chips.child(icon(glyph, px(11.), color));
    }
    if !snapshot.pulls.is_empty() {
        chips = chips.child(format!("PR {}", snapshot.pulls.len()));
    }
    if !snapshot.work_items.is_empty() {
        chips = chips.child(format!("WI {}", snapshot.work_items.len()));
    }
    Some(chips.into_any_element())
}

fn keycap(label: &'static str) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .h(px(17.))
        .min_w(px(18.))
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(theme::LINE_STRONG)
        .text_size(px(theme::font_label()))
        .text_color(theme::TEXT_MUTED)
        .child(label)
        .into_any_element()
}
