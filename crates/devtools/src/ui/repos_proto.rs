//! Prototype of the repos list redesign ("Ledger" direction).
//!
//! Rendered instead of the data table when `DEVTOOLS_UI_PROTO=1`, so the
//! shipping table stays untouched while the look is being judged. The point
//! of the spike is hierarchy, not behaviour: rows are two lines (identity +
//! context), the column chrome and the five launcher icons are gone, and the
//! only colour on a row is the branch, the change count and the favorite
//! star. Nothing here is wired to the store yet — clicks land in the next
//! pass, once the direction is picked.

use crate::store::Store;
use crate::store::projection::ProjectedRow;
use crate::theme;
use crate::ui::icon;
use devtools_core::repo_store::RepoEntry;
use gpui::prelude::*;
use gpui::{FontWeight, SharedString, div, px};

/// True when the prototype list should stand in for the table.
pub fn enabled() -> bool {
    std::env::var_os("DEVTOOLS_UI_PROTO").is_some()
}

/// The whole list: one quiet header per folder group, then its rows.
pub fn render(store: &Store) -> gpui::AnyElement {
    let mut list = div()
        .id("repos-proto-list")
        .flex()
        .flex_col()
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .pt_1();

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
                    list = list.child(repo_row(repo, row_ix));
                }
            }
        }
    }

    list.into_any_element()
}

/// A group band is a label, a count and a rule — no card, no chevron box.
fn group_band(key: &str, name: &str, count: usize, collapsed: bool) -> gpui::AnyElement {
    div()
        .id(SharedString::from(format!("proto-group-{key}")))
        .flex()
        .items_center()
        .gap_2()
        // aligns with the repo names, past the 16px star gutter
        .pl(px(44.))
        .pr_4()
        .pt_4()
        .pb_1p5()
        .child(
            div()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT_MUTED)
                .child(name.to_uppercase()),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::TEXT_FAINT)
                .child(format!("{count}")),
        )
        .when(collapsed, |row| {
            row.child(
                div()
                    .text_size(px(11.))
                    .text_color(theme::TEXT_FAINT)
                    .child("collapsed"),
            )
        })
        .child(div().flex_1().h(px(1.)).bg(theme::BORDER_LIGHT))
        .into_any_element()
}

/// One repository: identity on the first line, context on the second, the
/// state cluster on the right and the launchers only on hover.
fn repo_row(repo: &RepoEntry, row_ix: usize) -> gpui::AnyElement {
    let favorite = repo.is_favorite();
    let archived = repo.is_archived();
    let folder = repo.folder_path.to_string_lossy().to_string();
    let name_color = if archived {
        theme::TEXT_FAINT
    } else {
        theme::TEXT
    };

    let star = div()
        .flex_shrink_0()
        .size(px(16.))
        .flex()
        .items_center()
        .justify_center()
        .when(favorite, |el| {
            el.child(icon("icons/icon-star.svg", px(12.), theme::GOLD_BRIGHT))
        })
        .when(!favorite, |el| {
            el.opacity(0.)
                .group_hover("proto-repo-row", |s| s.opacity(0.55))
                .child(icon("icons/icon-star.svg", px(12.), theme::STAR_MUTED))
        });

    let branch = repo.git_branch_name.clone().unwrap_or_else(|| "—".into());
    let branch_chip = div()
        .flex()
        .items_center()
        .gap_1()
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .bg(theme::CHIP)
        .text_size(px(11.))
        .text_color(theme::TEXT_DIM)
        .child(icon(
            "icons/icon-git-branch.svg",
            px(10.),
            theme::TEXT_FAINT,
        ))
        .child(branch);

    let meta = format!(
        "{folder}  ·  {}",
        repo.git_last_commit_at
            .map(devtools_core::rel_time::format)
            .unwrap_or_else(|| "no commits".into())
    );

    let identity = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(theme::font_body() + 0.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(name_color)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(repo.name.clone()),
                )
                .child(branch_chip),
        )
        .child(
            div()
                .text_size(px(11.))
                .text_color(theme::TEXT_FAINT)
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(meta),
        );

    // a clean repo says nothing: the absence of a number is the signal
    let changes =
        div()
            .flex()
            .items_center()
            .justify_end()
            .gap_1()
            .when(repo.git_modified_count > 0, |el| {
                el.child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::AMBER_BRIGHT)
                        .child(format!("{}", repo.git_modified_count)),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme::TEXT_MUTED)
                        .child("changes"),
                )
            });

    let actions = div()
        .flex()
        .items_center()
        .gap_1()
        .opacity(0.)
        .group_hover("proto-repo-row", |s| s.opacity(1.))
        .child(action_icon(
            "icons/icon-agent-sparkle.svg",
            "Open in warp-agent",
        ))
        .child(action_icon("icons/icon-code.svg", "Open in VS Code"))
        .child(action_icon("icons/icon-folder.svg", "Open folder"))
        .child(action_icon("icons/icon-terminal.svg", "Open terminal"));

    div()
        .id(SharedString::from(format!("proto-repo-{row_ix}")))
        .group("proto-repo-row")
        .flex()
        .items_center()
        .gap_3()
        .h(px(58.))
        .px_4()
        .border_b_1()
        .border_color(theme::BORDER_LIGHT)
        .when(archived, |row| row.opacity(0.6))
        .hover(|s| s.bg(theme::HOVER))
        .child(star)
        .child(identity)
        .child(div().flex_shrink_0().w(px(72.)).child(changes))
        .child(div().flex_shrink_0().w(px(108.)).child(actions))
        .into_any_element()
}

fn action_icon(path: &'static str, label: &'static str) -> gpui::AnyElement {
    div()
        .id(SharedString::from(label))
        .size(px(24.))
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(theme::HOVER))
        .child(icon(path, px(13.), theme::TEXT_MUTED))
        .into_any_element()
}
