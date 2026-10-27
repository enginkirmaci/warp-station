//! The toolbar dialogs as centered overlay cards: Add Repositories (deep scan
//! with a pre-checked checklist; already-tracked repos locked) and Clone from
//! URL (auto-derived name, default destination from the first scan root).
//! Dialog-bound net results land on the store and are drained in the page's
//! render, the same pass that draws them.

use crate::store::Store;
use crate::theme;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::Sizable as _;

use crate::ui::repos::ReposPage;

pub enum Dialog {
    AddRepo(AddRepoDialog),
    Clone(CloneDialog),
}

impl Dialog {
    pub fn render(&mut self, window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        match self {
            Dialog::AddRepo(dialog) => dialog.render(window, cx),
            Dialog::Clone(dialog) => dialog.render(window, cx),
        }
    }
}

/// The last URL path segment minus .git — the clone's folder name.
pub fn derive_repo_name(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    let last = trimmed.rsplit(['/']).next().unwrap_or("");
    let last = last.strip_suffix(".git").unwrap_or(last);
    if last.is_empty() {
        "repo".to_string()
    } else {
        last.to_string()
    }
}

fn overlay_shell(
    id: &'static str,
    cx: &mut Context<ReposPage>,
    body: gpui::Div,
) -> gpui::AnyElement {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .absolute()
        .inset_0()
        .occlude()
        .bg(gpui::black().opacity(0.5))
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .on_click(cx.listener(|this: &mut ReposPage, _, _, cx| {
            this.dialog = None;
            cx.notify();
        }))
        .child(body.id("dialog-card-body").occlude().on_click(|_, _, _| {}))
        .into_any_element()
}

fn card(title: &str) -> gpui::Div {
    div()
        .min_w(px(480.))
        .rounded_xl()
        .border_1()
        .border_color(theme::HAIRLINE)
        .bg(theme::POPUP)
        .shadow_lg()
        .p_4()
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .text_size(px(theme::font_title()))
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.to_string()),
        )
}

fn plain_button(label: &str, on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static) -> gpui::AnyElement {
    let id = format!("btn-{label}");
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::font_small()))
        .text_color(theme::TEXT_DIM)
        .border_1()
        .border_color(theme::HAIRLINE)
        .hover(|s| s.bg(theme::HOVER))
        .on_click(on_click)
        .child(label.to_string())
        .into_any_element()
}

fn accent_button(
    label: &str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = format!("btn-accent-{label}");
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::font_small()))
        .font_weight(FontWeight::SEMIBOLD)
        .bg(theme::ACCENT)
        .text_color(theme::ON_ACCENT)
        .map(|d| {
            if enabled {
                d.hover(|s| s.opacity(0.9))
            } else {
                d.opacity(0.4)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}

pub struct AddRepoDialog {
    pub root: Entity<InputState>,
    pub scanning: bool,
    /// true once a scan has settled — distinguishes "never scanned" from
    /// "scanned and found nothing"
    pub scanned: bool,
    /// (repo, checked, already-tracked)
    pub found: Vec<(devtools_core::scan::ScannedRepo, bool, bool)>,
}

impl AddRepoDialog {
    pub fn new(window: &mut Window, cx: &mut Context<ReposPage>) -> Self {
        let root = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Folder to scan (e.g. /home/you/Developments)")
        });
        cx.subscribe(&root, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.kick_deep_scan(cx);
            }
            // the Scan button enables as soon as a path is typed
            cx.notify();
        })
        .detach();
        Self { root, scanning: false, scanned: false, found: Vec::new() }
    }

    fn render(&mut self, window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        let selectable = self.found.iter().filter(|(_, _, tracked)| !*tracked).count();
        let selected = self
            .found
            .iter()
            .filter(|(_, checked, tracked)| *checked && !*tracked)
            .count();
        let all_checked = selectable > 0 && self.found.iter().all(|(_, checked, tracked)| *tracked || *checked);
        // leave room for the card chrome, the footer and the window padding
        // so a long scan result list scrolls instead of pushing Add/Cancel out
        // of the dialog on short windows
        let list_max = (window.viewport_size().height.as_f32() - 520.).clamp(100., 280.);
        let mut list = div()
            .id("found-repo-list")
            .flex()
            .flex_col()
            .gap_1()
            .max_h(px(list_max))
            .overflow_y_scroll();
        for (index, (repo, checked, tracked)) in self.found.iter().enumerate() {
            let label: SharedString = if *tracked {
                format!("{}   (already tracked)", repo.name).into()
            } else {
                repo.name.clone().into()
            };
            list = list.child(
                div()
                    .id(("found-repo", index as u64))
                    .debug_selector(move || format!("found-repo-{index}"))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(theme::font_small()))
                    .when(*checked && !*tracked, |d| d.bg(theme::tint(theme::ACCENT)))
                    .when(*tracked, |d| d.opacity(0.5))
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(move |this: &mut ReposPage, _, _, cx| {
                        if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog
                            && !dialog.found[index].2 {
                                dialog.found[index].1 = !dialog.found[index].1;
                            }
                        cx.notify();
                    }))
                    .child(
                        // checkbox square
                        div()
                            .rounded_xs()
                            .size(px(13.))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_color(theme::TEXT_FAINT)
                            .when(*checked && !*tracked, |d| {
                                d.bg(theme::ACCENT).border_color(theme::ACCENT).child(icon(
                                    "icons/icon-check.svg",
                                    px(9.),
                                    theme::ON_ACCENT,
                                ))
                            }),
                    )
                    .child(label),
            );
        }

        let hint = if self.scanning {
            "Scanning…"
        } else if self.found.is_empty() && self.scanned {
            "No repositories found under that folder — try another folder, or raise the scan depth in Settings"
        } else if self.found.is_empty() {
            "Type a folder path and scan — searched up to the configured depth"
        } else if selectable == 0 {
            "All found repositories are already in your list."
        } else {
            "Click entries to toggle selection, then Add"
        };

        // Scan needs a path; Add needs at least one checked row. Keeping the
        // button disabled until then makes the no-op states obvious instead
        // of closing the dialog on a click that adds nothing.
        let can_scan = if self.found.is_empty() {
            !self.scanning && !self.root.read(cx).value().trim().is_empty()
        } else {
            selected > 0
        };

        let body = card("Add repositories")
            .child(
                // geometric marker for the probes: its bounds are the field's
                div()
                    .id("add-root-input")
                    .debug_selector(|| "add-root-input".into())
                    .child(Input::new(&self.root).small()),
            )
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(hint),
            )
            .when(!self.found.is_empty() && selectable > 0, |card| {
                card.child(
                    div()
                        .id("select-all-row")
                        .cursor_pointer()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_0p5()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_MUTED)
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(cx.listener(move |this: &mut ReposPage, _, _, cx| {
                            if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog {
                                // three-state header checkbox touches only the
                                // selectable rows
                                let target = !all_checked;
                                for (_, checked, tracked) in dialog.found.iter_mut() {
                                    if !*tracked {
                                        *checked = target;
                                    }
                                }
                            }
                            cx.notify();
                        }))
                        .child(
                            div()
                                .rounded_xs()
                                .size(px(13.))
                                .border_1()
                                .flex()
                                .items_center()
                                .justify_center()
                                .border_color(theme::TEXT_FAINT)
                                .when(all_checked, |d| {
                                    d.bg(theme::ACCENT).border_color(theme::ACCENT).child(icon(
                                        "icons/icon-check.svg",
                                        px(9.),
                                        theme::ON_ACCENT,
                                    ))
                                }),
                        )
                        .child(format!("Select all available ({selectable})")),
                )
            })
            .when(!self.found.is_empty(), |card| card.child(list))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    // once results are on screen the primary becomes Add; a
                    // visible Rescan keeps "scan a different folder" one click
                    // away instead of hiding it behind Enter-in-the-field
                    .when(self.scanned && !self.scanning, |row| {
                        row.child(plain_button(
                            "Rescan",
                            cx.listener(|this: &mut ReposPage, _, _, cx| this.kick_deep_scan(cx)),
                        ))
                    })
                    .child(plain_button("Cancel", cx.listener(|this: &mut ReposPage, _, _, cx| {
                        this.dialog = None;
                        cx.notify();
                    })))
                    .child(accent_button(
                        if self.found.is_empty() { "Scan" } else { "Add" },
                        can_scan,
                        cx.listener(|this: &mut ReposPage, _, _, cx| {
                            if let Some(Dialog::AddRepo(dialog)) = &this.dialog {
                                if dialog.found.is_empty() {
                                    this.kick_deep_scan(cx);
                                } else {
                                    this.commit_add_dialog(cx);
                                }
                            }
                        }),
                    )),
            );
        overlay_shell("add-repo-overlay", cx, body)
    }
}

pub struct CloneDialog {
    pub url: Entity<InputState>,
    pub destination: Entity<InputState>,
    pub busy: bool,
    pub error: Option<String>,
}

impl CloneDialog {
    pub fn new(store: &Entity<Store>, window: &mut Window, cx: &mut Context<ReposPage>) -> Self {
        let url = cx.new(|cx| {
            InputState::new(window, cx).placeholder("https://github.com/owner/repo.git")
        });
        cx.subscribe(&url, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.commit_clone(cx);
            }
            // the Clone button enables as soon as the URL has a usable shape
            cx.notify();
        })
        .detach();
        // default destination: the PARENT of the first existing scan
        // root — roots are repo folders themselves, and cloning into one would
        // nest a repo inside another repo's working tree
        let default_dest = store
            .read(cx)
            .settings()
            .repos
            .repo_scan_folders
            .iter()
            .flatten()
            .find(|f| std::path::Path::new(f).is_dir())
            .and_then(|f| {
                std::path::Path::new(f)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_default();
        let destination = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Destination folder (defaults to your first scan root's parent)")
                .default_value(default_dest)
        });
        Self { url, destination, busy: false, error: None }
    }

    pub fn derived_name(&self, cx: &Context<ReposPage>) -> String {
        derive_repo_name(self.url.read(cx).value().as_ref())
    }

    fn render(&mut self, _window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        let name = self.derived_name(cx);
        let error = self.error.clone();
        let busy = self.busy;
        let raw_url = self.url.read(cx).value().to_string();
        let url_valid = !raw_url.trim().is_empty()
            && (raw_url.contains('/') || raw_url.contains(':'));
        let dest = self.destination.read(cx).value().to_string();
        let target_line = if dest.trim().is_empty() {
            "Enter the destination folder above".to_string()
        } else {
            format!("Clones into {}/{}", dest.trim_end_matches('/'), name)
        };
        let body = card("Clone from URL")
            .child(
                div()
                    .id("clone-url-input")
                    .debug_selector(|| "clone-url-input".into())
                    .child(Input::new(&self.url).small()),
            )
            .child(
                div()
                    .id("clone-dest-input")
                    .debug_selector(|| "clone-dest-input".into())
                    .child(Input::new(&self.destination).small()),
            )
            .child(
                div()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT_MUTED)
                    .child(target_line),
            )
            .when_some(error, |card, error| {
                card.child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::DANGER)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(plain_button("Cancel", cx.listener(|this: &mut ReposPage, _, _, cx| {
                        this.dialog = None;
                        cx.notify();
                    })))
                    .child(accent_button(
                        if busy { "Cloning…" } else { "Clone" },
                        !busy && url_valid,
                        cx.listener(|this: &mut ReposPage, _, _, cx| this.commit_clone(cx)),
                    )),
            );
        overlay_shell("clone-overlay", cx, body)
    }
}

#[cfg(test)]
mod tests {
    use super::derive_repo_name;

    #[test]
    fn derives_names_from_url_shapes() {
        assert_eq!(derive_repo_name("https://github.com/owner/repo.git"), "repo");
        assert_eq!(derive_repo_name("https://github.com/owner/repo"), "repo");
        assert_eq!(derive_repo_name("git@github.com:owner/my-repo.git"), "my-repo");
        assert_eq!(derive_repo_name("https://dev.azure.com/org/proj/_git/Thing"), "Thing");
        assert_eq!(derive_repo_name("https://github.com/owner/repo.git/"), "repo");
        assert_eq!(derive_repo_name(""), "repo");
    }
}
