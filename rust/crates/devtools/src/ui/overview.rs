//! The Overview tab: the repo's README rendered as markdown (the kit's
//! TextView) with a details sidebar (GitHub owner/language/license/topics via
//! the gh details probe, plus the local folder). The README pane reads
//! readme.md from disk on load; editing/saving and the wand arrive with the
//! opencode wave.

use crate::store::Store;
use crate::theme;
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
                if let Some(folder) = selected.clone() {
                    let settings = store.read(cx).settings();
                    if settings.repos.enable_git_hub {
                        if let Some(gh) = devtools_core::exe::locate(&settings.repos.git_hub_executable) {
                            let _ = store
                                .read(cx)
                                .net
                                .send_blocking(crate::net::NetCommand::GitHubDetails {
                                    folder: std::path::PathBuf::from(folder),
                                    gh_exe: gh.to_string_lossy().to_string(),
                                });
                        }
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
        let has_readme = self.readme.is_some();

        // 60/40 like the C# overview: README pane + details sidebar
        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .child(
                div()
                    .id("readme-pane")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .p_3()
                    .when(has_readme, |pane| pane.child(render_markdown(self.readme.as_deref().unwrap_or(""))))
                    .when(!has_readme, |pane| {
                        pane.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .h_full()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(theme::FONT_BODY))
                                        .text_color(theme::TEXT_FAINT)
                                        .child("No README"),
                                ),
                        )
                    }),
            )
            .child(
                div()
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
                    .child(div().text_size(px(theme::FONT_TITLE)).font_weight(FontWeight::SEMIBOLD).child("Repository Details"))
                    .child(detail_row("Folder", folder.clone().unwrap_or_default()))
                    .when_some(entry.as_ref().and_then(|e| e.git_branch_name.clone()), |side, branch| {
                        side.child(detail_row("Branch", branch))
                    })
                    .when_some(details.as_ref(), |side, details| {
                        side
                            .when_some(details.url.clone(), |side, url| side.child(link_row("GitHub", url)))
                            .when_some(details.owner.clone(), |side, owner| side.child(detail_row("Owner", owner)))
                            .when_some(details.created_at.clone(), |side, created| {
                                side.child(detail_row("Created", date_only(&created)))
                            })
                            .when_some(details.primary_language.clone(), |side, lang| side.child(detail_row("Language", lang)))
                            .when_some(details.license.clone(), |side, license| side.child(detail_row("License", license)))
                            .when_some(details.default_branch.clone(), |side, branch| side.child(detail_row("Default branch", branch)))
                            .when(!details.topics.is_empty(), |side| {
                                side.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(div().text_size(px(theme::FONT_SMALL)).text_color(theme::TEXT_MUTED).child("Topics"))
                                        .child(
                                            div().flex().flex_row().flex_wrap().gap_1().children(
                                                details.topics.iter().map(|topic| {
                                                    div()
                                                        .rounded_md()
                                                        .bg(theme::tint(theme::BLUE_BRIGHT))
                                                        .px_1p5()
                                                        .text_size(px(theme::FONT_SMALL - 2.))
                                                        .text_color(theme::BLUE_BRIGHT)
                                                        .child(topic.clone())
                                                }),
                                            ),
                                        ),
                                )
                            })
                    }),
            )
    }
}

fn read_readme(folder: &str) -> Option<String> {
    for name in ["README.md", "readme.md", "Readme.md", "README.markdown", "README"] {
        if let Ok(text) = std::fs::read_to_string(std::path::Path::new(folder).join(name)) {
            return Some(text);
        }
    }
    None
}

fn date_only(iso: &str) -> String {
    iso.get(..10).unwrap_or(iso).to_string()
}

fn detail_row(label: &str, value: String) -> impl IntoElement + use<> {
    div()
        .flex()
        .flex_col()
        .gap_0p5()
        .child(div().text_size(px(theme::FONT_SMALL)).text_color(theme::TEXT_MUTED).child(label.to_string()))
        .child(
            div()
                .text_size(px(theme::FONT_SMALL))
                .text_color(theme::TEXT_DIM)
                .child(value),
        )
}

fn link_row(label: &str, url: String) -> impl IntoElement + use<> {
    let open_url = url.clone();
    div()
        .id(SharedString_from(label))
        .cursor_pointer()
        .flex()
        .flex_col()
        .gap_0p5()
        .on_click(move |_, _, _| {
            let _ = devtools_core::terminal::open_folder(&open_url);
        })
        .child(div().text_size(px(theme::FONT_SMALL)).text_color(theme::TEXT_MUTED).child(label.to_string()))
        .child(
            div()
                .text_size(px(theme::FONT_SMALL))
                .text_color(theme::ACCENT)
                .child(url),
        )
}

fn SharedString_from(label: &str) -> gpui::SharedString {
    gpui::SharedString::from(format!("detail-link-{label}"))
}

fn render_markdown(source: &str) -> impl IntoElement + use<> {
    TextView::markdown(
        gpui::ElementId::Name("readme".into()),
        gpui::SharedString::from(source.to_string()),
    )
    .selectable(true)
}
