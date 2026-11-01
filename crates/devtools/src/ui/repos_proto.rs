//! Prototype of the repos list redesign ("Ledger" direction).
//!
//! Rendered instead of the data table when `DEVTOOLS_UI_PROTO` is set, so the
//! shipping table stays untouched while the look is being judged. The point
//! of the spike is hierarchy, not behaviour: rows are two lines (identity +
//! context), the column chrome and the five launcher icons are gone, and the
//! only colour on a row is the branch, the change count and the favorite
//! star. Nothing here is wired to the store yet — clicks land in the next
//! pass, once the direction is picked.

use crate::store::Store;
use crate::store::projection::ProjectedRow;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use devtools_core::rel_time;
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::{Entity, FontWeight, Rgba, SharedString, div, px};

/// The whole list: one quiet header per folder group, then its rows. Rows
/// select on click — the rail beside them is useless otherwise.
pub fn render(store: &Store, store_entity: &Entity<Store>) -> gpui::AnyElement {
    let mut list = div()
        .id("repos-proto-list")
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .pt_1();

    // The rail previews the first row when nothing is selected, so the list
    // has to paint that same row as selected — otherwise the two panes
    // disagree about which repo the rail describes.
    let focused = crate::ui::inspector::focused_entry(store)
        .map(|repo| repo.folder_path.to_string_lossy().to_string());

    for (row_ix, row) in store.projected.iter().enumerate() {
        match row {
            ProjectedRow::Group {
                key,
                name,
                count,
                collapsed,
            } => list = list.child(group_band(key, name, *count, *collapsed)),
            ProjectedRow::Repo(ix) => {
                if let Some(repo) = store.repos.get(*ix) {
                    let folder = repo.folder_path.to_string_lossy();
                    let selected = focused.as_deref() == Some(folder.as_ref());
                    list = list.child(repo_row(repo, row_ix, selected, store_entity));
                }
            }
        }
    }

    list.into_any_element()
}

/// A folder band: the export's 30px row on its own step of the ramp, with the
/// sort hint pushed to the right edge.
fn group_band(key: &str, name: &str, count: usize, collapsed: bool) -> gpui::AnyElement {
    div()
        .id(SharedString::from(format!("proto-group-{key}")))
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(30.))
        .px(px(20.))
        .border_b_1()
        .border_color(theme::BORDER_LIGHT)
        .bg(theme::GROUP_BG)
        .child(icon(
            if collapsed {
                "icons/icon-chevron-right.svg"
            } else {
                "icons/icon-chevron-down.svg"
            },
            px(13.),
            theme::MUTED_ICON,
        ))
        .child(icon("icons/icon-folder.svg", px(13.), theme::MUTED_ICON))
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
        .child(div().flex_1())
        .child(
            div()
                .text_size(px(theme::font_mono_meta()))
                .text_color(theme::TEXT_MUTED)
                .child("attention first"),
        )
        .into_any_element()
}

/// One repository: identity on the first line, context on the second, the
/// activity stamp on the right and the open hint only while hovered.
fn repo_row(
    repo: &RepoEntry,
    row_ix: usize,
    selected: bool,
    store: &Entity<Store>,
) -> gpui::AnyElement {
    let favorite = repo.is_favorite();
    let archived = repo.is_archived();
    let name_color = if archived {
        theme::TEXT_MUTED
    } else {
        theme::TEXT
    };
    let select_target = store.clone();
    let select_folder = repo.folder_path.to_string_lossy().to_string();

    let star = div()
        .flex_shrink_0()
        .size(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .when(favorite, |el| {
            el.child(icon("icons/icon-star-filled.svg", px(13.), theme::STAR_ON))
        })
        .when(!favorite, |el| {
            el.opacity(0.62)
                .group_hover("proto-repo-row", |s| s.opacity(1.))
                .child(icon("icons/icon-star.svg", px(13.), theme::STAR_MUTED))
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
    if repo.git_modified_count > 0 {
        meta = meta.child(counted(
            "icons/icon-file-diff.svg",
            repo.git_modified_count,
            "changes",
            theme::AMBER_BRIGHT,
        ));
    }
    if repo.github_issue_count > 0 {
        meta = meta.child(counted(
            "icons/icon-circle-dot.svg",
            repo.github_issue_count,
            "issues",
            theme::TEXT_DIM,
        ));
    }
    if repo.git_to_push_count > 0 {
        meta = meta.child(
            div()
                .text_color(theme::AMBER_BRIGHT)
                .child(format!("↑{}", repo.git_to_push_count)),
        );
    }
    if repo.git_modified_count == 0 && repo.github_issue_count == 0 && repo.git_to_push_count == 0 {
        meta = meta.child(
            div()
                .flex()
                .items_center()
                .gap(px(4.))
                .text_color(theme::SUCCESS_BRIGHT)
                .child(icon("icons/icon-check.svg", px(11.), theme::SUCCESS_BRIGHT))
                .child("clean"),
        );
    }

    let identity = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .text_size(px(theme::font_repo_name()))
                .font_weight(FontWeight::MEDIUM)
                .text_color(name_color)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(repo.name.clone()),
        )
        .child(meta);

    // the mockup's stamp is compact ("12m", "1d"); rel_time says "… ago"
    let stamp = repo
        .git_last_commit_at
        .map(|at| rel_time::format(at).trim_end_matches(" ago").to_string())
        .unwrap_or_else(|| "—".into());

    let row = div()
        .id(SharedString::from(format!("proto-repo-{row_ix}")))
        .group("proto-repo-row")
        .a11y_button(repo.name.clone())
        .cursor_pointer()
        .on_click(move |_, _, cx| {
            select_target.update(cx, |s, cx| s.select_repo(Some(select_folder.clone()), cx));
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
                .group_hover("proto-repo-row", |s| s.opacity(0.))
                .child(stamp),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(18.))
                .opacity(0.)
                .group_hover("proto-repo-row", |s| s.opacity(1.))
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
        .child(div().text_color(tint).child(noun))
        .into_any_element()
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
