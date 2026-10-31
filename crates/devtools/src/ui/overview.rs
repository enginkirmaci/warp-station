//! The Overview tab: the repo's README rendered as markdown (the kit's
//! TextView) with a details sidebar (GitHub owner/language/license/topics via
//! the gh details probe, plus the local folder). The wand generates a draft
//! that renders in place with Save/Discard; Save writes README.md.

use crate::net::WandKind;
use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px, relative};
use gpui_kit::base::text::TextView;

pub struct OverviewView {
    pub store: Entity<Store>,
    /// SharedString: every render hands the TextView a refcount, never a
    /// full copy of the README.
    readme: Option<SharedString>,
    readme_loaded_for: Option<String>,
}

impl OverviewView {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |this, store, cx| {
            let selected = store.read(cx).selected_repo.clone();
            if selected != this.readme_loaded_for {
                this.readme = selected
                    .as_ref()
                    .and_then(|folder| read_readme(folder))
                    .map(SharedString::from);
                this.readme_loaded_for = selected.clone();
                // details probe rides along with the pane's first open
                if let Some(folder) = selected {
                    let settings = store.read(cx).settings();
                    if settings.repos.enable_git_hub
                        && let Some(gh) =
                            devtools_core::exe::locate(&settings.repos.git_hub_executable)
                    {
                        let _ = store.read(cx).net.send_blocking(
                            crate::net::NetCommand::GitHubDetails {
                                folder: std::path::PathBuf::from(folder),
                                gh_exe: gh.to_string_lossy().to_string(),
                            },
                        );
                    }
                }
                cx.notify();
            }
        })
        .detach();
        Self {
            store,
            readme: None,
            readme_loaded_for: None,
        }
    }
}

impl Render for OverviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let details = self.store.read(cx).github_details.clone();
        let entry = self.store.read(cx).selected_entry().cloned();
        let draft = self.store.read(cx).readme_draft.clone();
        let wand_active = {
            let state = self.store.read(cx);
            state.wand_busy == Some(WandKind::Readme)
                && state.wand_repo.as_deref() == state.selected_repo.as_deref()
        };
        let ai_available = self.store.read(cx).wand_available();
        let has_draft = draft.is_some();
        let shown: Option<SharedString> = draft
            .as_ref()
            .map(|d| SharedString::from(d.as_str()))
            .or_else(|| self.readme.clone());

        let header = div()
            .id("readme-header")
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(theme::BORDER_LIGHT)
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(if has_draft {
                        "README.md (unsaved)"
                    } else {
                        "README.md"
                    }),
            )
            .child(div().flex_1())
            .when(has_draft, |row| {
                let folder_save = folder.clone();
                row.child(
                    div()
                        .id("readme-discard")
                        .a11y_button("Discard README draft")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_0p5()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_DIM)
                        .border_1()
                        .border_color(theme::HAIRLINE)
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.store.update(cx, |s: &mut Store, cx| {
                                s.readme_draft = None;
                                cx.notify();
                            });
                        }))
                        .child(icon(
                            "icons/icon-undo.svg",
                            px(theme::icon_small() - 1.),
                            theme::TEXT_DIM,
                        ))
                        .child("Discard"),
                )
                .child(
                    div()
                        .id("readme-save")
                        .a11y_button("Save README")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_0p5()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_small()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(theme::ACCENT)
                        .text_color(theme::ON_ACCENT)
                        .hover(|s| s.opacity(0.9))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let Some(folder) = folder_save.clone() else {
                                return;
                            };
                            let draft = this
                                .store
                                .update(cx, |s: &mut Store, _| s.readme_draft.take());
                            if let Some(draft) = draft {
                                let path = std::path::Path::new(&folder).join("README.md");
                                match std::fs::write(&path, format!("{draft}\n")) {
                                    Ok(()) => {
                                        this.readme = Some(draft.into());
                                        tracing::info!("saved {}", path.display());
                                    }
                                    Err(err) => {
                                        tracing::warn!("README save failed: {err}");
                                        this.store.update(cx, |s: &mut Store, cx| {
                                            s.notify_error("Could not save README.md");
                                            cx.notify();
                                        });
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .child(icon(
                            "icons/icon-save.svg",
                            px(theme::icon_small() - 1.),
                            theme::ON_ACCENT,
                        ))
                        .child("Save"),
                )
            })
            .when(!wand_active && shown.is_some() && ai_available, |row| {
                row.child({
                    let store = self.store.clone();
                    div()
                        .id("readme-wand")
                        .a11y_button("Draft README with OpenCode")
                        .cursor_pointer()
                        .rounded_md()
                        .p_1()
                        .flex()
                        .items_center()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, cx| {
                            store.update(cx, |s, cx| s.run_wand(WandKind::Readme, cx));
                        })
                        .child(icon(
                            "icons/icon-wand.svg",
                            px(theme::icon_small()),
                            theme::TEXT_MUTED,
                        ))
                })
            })
            .when(wand_active, |row| {
                row.child(div().rounded_md().p_1().flex().items_center().child(icon(
                    "icons/icon-wand.svg",
                    px(theme::icon_small()),
                    theme::ACCENT,
                )))
            });

        let store_for_empty = self.store.clone();
        let pane = div()
            .id("readme-pane")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .px_5()
            .py_4()
            .when(shown.is_some(), |pane| {
                pane.child(render_markdown(shown.clone().unwrap_or_default()))
            })
            .when(shown.is_none(), |pane| {
                pane.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .h_full()
                        .child(icon(
                            "icons/icon-note.svg",
                            px(theme::icon_large() + 10.),
                            theme::TEXT_FAINT,
                        ))
                        .child(
                            div()
                                .text_size(px(theme::font_body() + 1.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme::TEXT_DIM)
                                .child("No README yet"),
                        )
                        .child(
                            div()
                                .text_size(px(theme::font_small()))
                                .text_color(theme::TEXT_FAINT)
                                .child("Create a README.md in the repository root to see it here."),
                        )
                        .when(ai_available && !wand_active, |column| {
                            column.child(
                                div()
                                    .id("readme-empty-wand")
                                    .a11y_button("Draft README with OpenCode")
                                    .cursor_pointer()
                                    .rounded_md()
                                    .border_1()
                                    .border_color(theme::ACCENT)
                                    .px_3()
                                    .py_1p5()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .text_size(px(theme::font_small()))
                                    .text_color(theme::ACCENT)
                                    .hover(|s| s.bg(theme::tint(theme::ACCENT)))
                                    .on_click(move |_, _, cx| {
                                        store_for_empty
                                            .update(cx, |s, cx| s.run_wand(WandKind::Readme, cx));
                                    })
                                    .child(icon(
                                        "icons/icon-wand.svg",
                                        px(theme::icon_small()),
                                        theme::ACCENT,
                                    ))
                                    .child("Draft with OpenCode"),
                            )
                        }),
                )
            });

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
                    .child(header)
                    .child(pane),
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
            side = side.child(detail_row("Created", format_created(created)));
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

fn read_readme(folder: &str) -> Option<String> {
    for name in [
        "README.md",
        "readme.md",
        "Readme.md",
        "README.markdown",
        "README",
    ] {
        if let Ok(text) = std::fs::read_to_string(std::path::Path::new(folder).join(name)) {
            return Some(text);
        }
    }
    None
}

/// gh returns ISO timestamps; the details card shows "27 Nov 2023".
fn format_created(iso: &str) -> String {
    chrono::NaiveDate::parse_from_str(&iso[..10.min(iso.len())], "%Y-%m-%d")
        .map(|d| d.format("%-d %b %Y").to_string())
        .unwrap_or_else(|_| iso.to_string())
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

fn render_markdown(source: SharedString) -> impl IntoElement + use<> {
    TextView::markdown(gpui::ElementId::Name("readme".into()), source).selectable(true)
}
