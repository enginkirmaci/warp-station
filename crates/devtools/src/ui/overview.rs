//! The Overview tab: the repo's README rendered as markdown (the kit's
//! TextView) with a details sidebar (GitHub owner/language/license/topics via
//! the gh details probe, plus the local folder). The wand generates a draft
//! that renders in place with Save/Discard; Save writes README.md.

use crate::net::WandKind;
use crate::store::{BottomBarTab, Store};
use crate::theme;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, Window};
use gpui_kit::base::text::TextView;

pub struct OverviewView {
    pub store: Entity<Store>,
    readme: Option<String>,
    readme_loaded_for: Option<String>,
}

impl OverviewView {
    pub fn new(store: Entity<Store>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |this, store, cx| {
            let selected = store.read(cx).selected_repo.clone();
            if selected != this.readme_loaded_for {
                this.readme = selected.as_ref().and_then(|folder| read_readme(folder));
                this.readme_loaded_for = selected.clone();
                // details probe rides along with the pane's first open
                if let Some(folder) = selected {
                    let settings = store.read(cx).settings();
                    if settings.repos.enable_git_hub
                        && let Some(gh) = devtools_core::exe::locate(&settings.repos.git_hub_executable) {
                            let _ = store
                                .read(cx)
                                .net
                                .send_blocking(crate::net::NetCommand::GitHubDetails {
                                    folder: std::path::PathBuf::from(folder),
                                    gh_exe: gh.to_string_lossy().to_string(),
                                });
                        }
                }
                cx.notify();
            }
        })
        .detach();
        Self { store, readme: None, readme_loaded_for: None }
    }
}

impl Render for OverviewView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let folder = self.store.read(cx).selected_repo.clone();
        let details = self.store.read(cx).github_details.clone();
        let entry = self.store.read(cx).selected_entry().cloned();
        let draft = self.store.read(cx).readme_draft.clone();
        let wand_active = self.store.read(cx).wand_busy == Some(WandKind::Readme);
        let has_draft = draft.is_some();
        let shown = draft.clone().or_else(|| self.readme.clone());

        let header = div()
            .id("readme-header")
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(if has_draft { "README draft (unsaved)" } else { "README" }),
            )
            .child(div().flex_1())
            .when(has_draft, |row| {
                let folder_save = folder.clone();
                row.child(
                    div()
                        .id("readme-discard")
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
                        .child(icon("icons/icon-undo.svg", px(theme::icon_small() - 1.), theme::TEXT_DIM))
                        .child("Discard"),
                )
                .child(
                    div()
                        .id("readme-save")
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
                            let Some(folder) = folder_save.clone() else { return };
                            let draft = this.store.update(cx, |s: &mut Store, _| s.readme_draft.take());
                            if let Some(draft) = draft {
                                let path = std::path::Path::new(&folder).join("README.md");
                                match std::fs::write(&path, format!("{draft}\n")) {
                                    Ok(()) => {
                                        this.readme = Some(draft);
                                        tracing::info!("saved {}", path.display());
                                    }
                                    Err(err) => {
                                        tracing::warn!("README save failed: {err}");
                                        this.store.update(cx, |s: &mut Store, cx| {
                                            s.notice = Some((
                                                "Could not save README.md".into(),
                                                std::time::Instant::now(),
                                            ));
                                            cx.notify();
                                        });
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .child(icon("icons/icon-save.svg", px(theme::icon_small() - 1.), theme::ON_ACCENT))
                        .child("Save"),
                )
            })
            .when(!wand_active && self.store.read(cx).settings().opencode.enable_opencode, |row| {
                row.child({
                    let store = self.store.clone();
                    div()
                        .id("readme-wand")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_0p5()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_MUTED)
                        .border_1()
                        .border_color(theme::HAIRLINE)
                        .hover(|s| s.opacity(0.85))
                        .on_click(move |_, _, cx| {
                            store.update(cx, |s, cx| s.run_wand(WandKind::Readme, cx));
                        })
                        .child(icon("icons/icon-wand.svg", px(theme::icon_small() - 1.), theme::TEXT_MUTED))
                        .child("generate")
                })
            })
            .when(wand_active, |row| {
                row.child(
                    div()
                        .rounded_md()
                        .px_2()
                        .py_0p5()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::ACCENT)
                        .border_1()
                        .border_color(theme::tint(theme::ACCENT))
                        .child(icon("icons/icon-wand.svg", px(theme::icon_small() - 1.), theme::ACCENT))
                        .child("generating…"),
                )
            });

        let pane = div()
            .id("readme-pane")
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .p_3()
            .when(shown.is_some(), |pane| {
                pane.child(render_markdown(shown.as_deref().unwrap_or("")))
            })
            .when(shown.is_none(), |pane| {
                pane.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .h_full()
                        .child(
                            div()
                                .text_size(px(theme::font_body()))
                                .text_color(theme::TEXT_FAINT)
                                .child("No README"),
                        ),
                )
            });

        // 60/40: README column + details sidebar
        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .child(
                div()
                    .id("readme-column")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .child(header)
                    .child(pane),
            )
            .child(render_sidebar(folder, entry, details, self.store.clone(), cx))
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
    // three stat tiles: each opens its tab
    let tile = |id: &'static str, label: &'static str, count: u32, color: gpui::Rgba, tab: BottomBarTab| {
        let store = store.clone();
        div()
            .id(id)
            .cursor_pointer()
            .when(count > 0, |d| d.cursor_pointer())
            .flex_1()
            .flex()
            .flex_col()
            .gap_0p5()
            .rounded_md()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::STRONG_BACKGROUND)
            .p_2()
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
                    .gap_1()
                    .text_size(px(theme::font_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(format!("{count}")),
            )
            .child(
                div()
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
        .w(px(300.))
        .h_full()
        .flex()
        .flex_col()
        .gap_2()
        .p_3()
        .border_l_1()
        .border_color(theme::HAIRLINE)
        .overflow_y_scroll()
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(tile("tile-changes", "Changes", modified, theme::AMBER_BRIGHT, BottomBarTab::Changes))
                .when(settings.repos.enable_git_hub, |row| {
                    row.child(tile(
                        "tile-prs",
                        "Pull Requests",
                        prs,
                        theme::PURPLE_BRIGHT,
                        BottomBarTab::PullRequests,
                    ))
                    .child(tile("tile-issues", "Issues", issues, theme::SUCCESS_BRIGHT, BottomBarTab::Issues))
                }),
        )
        .child(
            div()
                .text_size(px(theme::font_title()))
                .font_weight(FontWeight::SEMIBOLD)
                .child("Repository Details"),
        )
        .child(detail_row("Folder", folder.unwrap_or_default()));
    if let Some(branch) = entry.and_then(|e| e.git_branch_name) {
        side = side.child(detail_row("Branch", branch));
    }
    if let Some(details) = &details {
        if let Some(url) = &details.url {
            side = side.child(link_row("GitHub", url.clone()));
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
            side = side.child(detail_row("Default branch", branch.clone()));
        }
        if !details.topics.is_empty() {
            side = side.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::TEXT_MUTED)
                            .child("Topics"),
                    )
                    .child(
                        div().flex().flex_row().flex_wrap().gap_1().children(
                            details.topics.iter().map(|topic| {
                                div()
                                    .rounded_md()
                                    .bg(theme::tint(theme::BLUE_BRIGHT))
                                    .px_1p5()
                                    .text_size(px(theme::font_small() - 2.))
                                    .text_color(theme::BLUE_BRIGHT)
                                    .child(topic.clone())
                            }),
                        ),
                    ),
            );
        }
    }
    side.into_any_element()
}

fn read_readme(folder: &str) -> Option<String> {
    for name in ["README.md", "readme.md", "Readme.md", "README.markdown", "README"] {
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

fn detail_row(label: &str, value: String) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(div().text_size(px(theme::font_small())).text_color(theme::TEXT_MUTED).child(label.to_string()))
        .child(
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_DIM)
                .child(value),
        )
}

fn link_row(label: &str, url: String) -> impl IntoElement + use<> {
    let open_url = url.clone();
    div()
        .id(gpui::SharedString::from(format!("detail-link-{label}")))
        .cursor_pointer()
        .flex()
        .flex_col()
        .gap_0p5()
        .on_click(move |_, _, _| {
            let _ = devtools_core::terminal::open_folder(&open_url);
        })
        .child(div().text_size(px(theme::font_small())).text_color(theme::TEXT_MUTED).child(label.to_string()))
        .child(
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::ACCENT)
                .child(url),
        )
}

fn render_markdown(source: &str) -> impl IntoElement + use<> {
    TextView::markdown(
        gpui::ElementId::Name("readme".into()),
        gpui::SharedString::from(source.to_string()),
    )
    .selectable(true)
}
