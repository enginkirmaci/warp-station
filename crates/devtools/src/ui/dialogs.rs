//! The toolbar dialogs as centered overlay cards: Add Repositories (deep scan
//! with a pre-checked checklist; already-tracked repos locked) and Clone from
//! URL (auto-derived name, default destination from the first scan root).
//! Dialog-bound net results land on the store and are drained in the page's
//! render, the same pass that draws them.

use crate::store::Store;
use crate::theme;
use crate::ui::A11y;
use crate::ui::icon;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::input::{Input, InputEvent, InputState};

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
        .a11y_button("Close dialog")
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
        .child(
            body.id("dialog-card-body")
                .role(gpui::Role::Dialog)
                .occlude()
                .on_click(|_, _, _| {}),
        )
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

fn plain_button(
    label: &str,
    enabled: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let id = format!("btn-{label}");
    div()
        .id(SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .a11y_button(label.to_string())
        .cursor_pointer()
        .rounded_md()
        .px_3()
        .py_1()
        .text_size(px(theme::font_small()))
        .text_color(theme::TEXT_DIM)
        .border_1()
        .border_color(theme::HAIRLINE)
        .map(|d| {
            if enabled {
                d.hover(|s| s.bg(theme::HOVER))
            } else {
                d.opacity(0.4)
            }
        })
        .when(enabled, |d| d.on_click(on_click))
        .child(label.to_string())
        .into_any_element()
}

/// The dialog card whose header carries a leading icon and an X close
/// button — the Add-Repositories mockup's chrome.
fn card_with_close(
    title: &str,
    on_close: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Div {
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
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(icon(
                    "icons/icon-plus.svg",
                    px(theme::icon_medium()),
                    theme::TEXT,
                ))
                .child(
                    div()
                        .flex_1()
                        .text_size(px(theme::font_title()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                )
                .child(
                    div()
                        .id("add-dialog-close")
                        .debug_selector(|| "add-dialog-close".into())
                        .a11y_button("Close")
                        .cursor_pointer()
                        .rounded_md()
                        .size(px(26.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(on_close)
                        .child(icon(
                            "icons/icon-close.svg",
                            px(theme::icon_small()),
                            theme::TEXT_MUTED,
                        )),
                ),
        )
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
        .a11y_button(label.to_string())
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
    /// Scan depth for this dialog, seeded from the settings; changing it here
    /// does not persist (Settings owns the stored value).
    pub depth: i32,
    pub scanning: bool,
    /// true once a scan has settled — distinguishes "never scanned" from
    /// "scanned and found nothing"
    pub scanned: bool,
    /// (repo, checked, already-tracked)
    pub found: Vec<(devtools_core::scan::ScannedRepo, bool, bool)>,
}

impl AddRepoDialog {
    pub fn new(window: &mut Window, depth: i32, cx: &mut Context<ReposPage>) -> Self {
        let root =
            cx.new(|cx| InputState::new(window, cx).placeholder("Pick or type a folder path…"));
        cx.subscribe(&root, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.kick_deep_scan(cx);
            }
            // the Scan button enables as soon as a path is typed
            cx.notify();
        })
        .detach();
        Self {
            root,
            depth,
            scanning: false,
            scanned: false,
            found: Vec::new(),
        }
    }

    fn render(&mut self, window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        let selectable = self
            .found
            .iter()
            .filter(|(_, _, tracked)| !*tracked)
            .count();
        let selected = self
            .found
            .iter()
            .filter(|(_, checked, tracked)| *checked && !*tracked)
            .count();
        let all_checked = selectable > 0
            && self
                .found
                .iter()
                .all(|(_, checked, tracked)| *tracked || *checked);

        // ---- results card -------------------------------------------------
        // leave room for the card chrome and the footer so a long scan result
        // list scrolls inside the well instead of pushing Cancel/Add out of
        // the dialog on short windows
        let results_h = (window.viewport_size().height.as_f32() - 400.).clamp(160., 400.);
        let card_center_text = if self.scanning {
            "Scanning…"
        } else if self.found.is_empty() && self.scanned {
            "No repositories found under that folder — try another folder, or raise the depth"
        } else {
            "Scan a folder to find git repositories."
        };

        let mut rows = div().flex().flex_col().gap_1();
        if selectable > 0 {
            rows = rows.child(
                div()
                    .id("select-all-row")
                    .a11y_toggle(format!("Select all available ({selectable})"), all_checked)
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
            );
        }
        for (index, (repo, checked, tracked)) in self.found.iter().enumerate() {
            let label: SharedString = if *tracked {
                format!("{}   (already tracked)", repo.name).into()
            } else {
                repo.name.clone().into()
            };
            rows = rows.child(
                div()
                    .id(("found-repo", index as u64))
                    .debug_selector(move || format!("found-repo-{index}"))
                    .a11y_toggle(label.to_string(), *checked && !*tracked)
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
                            && !dialog.found[index].2
                        {
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

        let results_card = div()
            .id("add-results")
            .debug_selector(|| "add-results".into())
            .h(px(results_h))
            .rounded_lg()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::STRONG_BACKGROUND)
            .overflow_hidden()
            .when(self.found.is_empty(), |card| {
                card.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(theme::font_body()))
                        .text_color(theme::TEXT_FAINT)
                        .child(card_center_text),
                )
            })
            .when(!self.found.is_empty(), |card| {
                card.child(
                    div()
                        .id("found-repo-list")
                        .h_full()
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .py_1()
                        .child(rows)
                        .when(selectable == 0, |column| {
                            column.child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .text_size(px(theme::font_small()))
                                    .text_color(theme::TEXT_FAINT)
                                    .child("All found repositories are already in your list."),
                            )
                        }),
                )
            });

        // ---- folder row + scan controls -----------------------------------
        let can_scan = !self.scanning && !self.root.read(cx).value().trim().is_empty();
        let depth_stepper = div()
            .h(px(30.))
            .flex()
            .flex_row()
            .items_center()
            .rounded_md()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::INPUT)
            .child(
                div()
                    .w(px(26.))
                    .flex()
                    .justify_center()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::TEXT)
                    .child(format!("{}", self.depth)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .border_l_1()
                    .border_color(theme::HAIRLINE)
                    .child(
                        div()
                            .id("depth-up")
                            .a11y_button("Increase scan depth")
                            .cursor_pointer()
                            .w(px(18.))
                            .h(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(cx.listener(|this: &mut ReposPage, _, _, cx| {
                                if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog {
                                    dialog.depth = (dialog.depth + 1).min(8);
                                    cx.notify();
                                }
                            }))
                            .child(icon("icons/icon-arrow-up.svg", px(8.), theme::TEXT_MUTED)),
                    )
                    .child(
                        div()
                            .id("depth-down")
                            .a11y_button("Decrease scan depth")
                            .cursor_pointer()
                            .w(px(18.))
                            .h(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(cx.listener(|this: &mut ReposPage, _, _, cx| {
                                if let Some(Dialog::AddRepo(dialog)) = &mut this.dialog {
                                    dialog.depth = (dialog.depth - 1).max(1);
                                    cx.notify();
                                }
                            }))
                            .child(icon("icons/icon-arrow-down.svg", px(8.), theme::TEXT_MUTED)),
                    ),
            );

        let body = card_with_close(
            "Add Repositories",
            cx.listener(|this: &mut ReposPage, _, _, cx| {
                this.dialog = None;
                cx.notify();
            }),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_MUTED)
                        .child("Folder to scan"),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(
                            div()
                                .id("add-root-input")
                                .debug_selector(|| "add-root-input".into())
                                .flex_1()
                                .child(Input::new(&self.root)),
                        )
                        .child(plain_button(
                            "Browse…",
                            !self.scanning,
                            cx.listener(|this: &mut ReposPage, _, _, cx| {
                                this.store.update(cx, |s, _| {
                                    let _ =
                                        s.net.send_blocking(crate::net::NetCommand::BrowseFolder);
                                });
                            }),
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(plain_button(
                    "Scan",
                    can_scan,
                    cx.listener(|this: &mut ReposPage, _, _, cx| this.kick_deep_scan(cx)),
                ))
                .child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_MUTED)
                        .child("Depth"),
                )
                .child(depth_stepper)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_FAINT)
                        .child("Pick or type a folder, then scan it for git repositories"),
                ),
        )
        .child(results_card)
        .child(
            div()
                .flex()
                .flex_row()
                .justify_end()
                .gap_2()
                .child(plain_button(
                    "Cancel",
                    true,
                    cx.listener(|this: &mut ReposPage, _, _, cx| {
                        this.dialog = None;
                        cx.notify();
                    }),
                ))
                .child(accent_button(
                    "Add",
                    !self.scanning && selected > 0,
                    cx.listener(|this: &mut ReposPage, _, _, cx| this.commit_add_dialog(cx)),
                )),
        );
        overlay_shell("add-repo-overlay", cx, body)
    }
}

pub struct CloneDialog {
    pub url: Entity<InputState>,
    /// The clone's folder name; left empty it follows the URL's derivation
    /// (the field's "repo" placeholder shows that fallback).
    pub name: Entity<InputState>,
    pub destination: Entity<InputState>,
    pub busy: bool,
    pub error: Option<String>,
}

impl CloneDialog {
    pub fn new(store: &Entity<Store>, window: &mut Window, cx: &mut Context<ReposPage>) -> Self {
        let url = cx
            .new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repo.git"));
        cx.subscribe(&url, |this: &mut ReposPage, _, event: &InputEvent, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.commit_clone(cx);
            }
            // the Clone button enables as soon as the URL has a usable shape
            cx.notify();
        })
        .detach();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("repo"));
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
                .placeholder("path to clone into")
                .default_value(default_dest)
        });
        Self {
            url,
            name,
            destination,
            busy: false,
            error: None,
        }
    }

    /// The effective clone folder: the typed Folder Name wins; otherwise the
    /// name derives from the URL, so pasting a link is enough.
    pub fn folder_name(&self, cx: &Context<ReposPage>) -> String {
        let typed = self.name.read(cx).value().trim().to_string();
        if typed.is_empty() {
            derive_repo_name(self.url.read(cx).value().as_ref())
        } else {
            typed
        }
    }

    fn render(&mut self, _window: &mut Window, cx: &mut Context<ReposPage>) -> gpui::AnyElement {
        let name = self.folder_name(cx);
        let error = self.error.clone();
        let busy = self.busy;
        let raw_url = self.url.read(cx).value().to_string();
        let url_valid =
            !raw_url.trim().is_empty() && (raw_url.contains('/') || raw_url.contains(':'));
        let dest = self.destination.read(cx).value().to_string();
        let target_line = if dest.trim().is_empty() {
            "Enter the destination folder above".to_string()
        } else {
            format!("Clones into {}/{}", dest.trim_end_matches('/'), name)
        };
        let body = card("Clone from URL")
            .child(form_field(
                "clone-url-input",
                "Repository URL",
                Input::new(&self.url),
            ))
            .child(form_field(
                "clone-name-input",
                "Folder Name",
                Input::new(&self.name),
            ))
            .child(form_field(
                "clone-dest-input",
                "Destination Folder",
                Input::new(&self.destination),
            ))
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
                    .child(plain_button(
                        "Cancel",
                        true,
                        cx.listener(|this: &mut ReposPage, _, _, cx| {
                            this.dialog = None;
                            cx.notify();
                        }),
                    ))
                    .child(accent_button(
                        if busy { "Cloning…" } else { "Clone" },
                        !busy && url_valid,
                        cx.listener(|this: &mut ReposPage, _, _, cx| this.commit_clone(cx)),
                    )),
            );
        overlay_shell("clone-overlay", cx, body)
    }
}

/// A labeled form field: quiet label over a full-size input — the clone
/// dialog's stacked-field layout. The id/selector wraps the INPUT only (its
/// bounds are the click-to-focus target; a label-plus-input wrapper would
/// put the center between the two).
fn form_field(id: &'static str, label: &str, input: Input) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .gap_1p5()
        .child(
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_MUTED)
                .child(label.to_string()),
        )
        .child(div().id(id).debug_selector(move || id.into()).child(input))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::derive_repo_name;

    #[test]
    fn derives_names_from_url_shapes() {
        assert_eq!(
            derive_repo_name("https://github.com/owner/repo.git"),
            "repo"
        );
        assert_eq!(derive_repo_name("https://github.com/owner/repo"), "repo");
        assert_eq!(
            derive_repo_name("git@github.com:owner/my-repo.git"),
            "my-repo"
        );
        assert_eq!(
            derive_repo_name("https://dev.azure.com/org/proj/_git/Thing"),
            "Thing"
        );
        assert_eq!(
            derive_repo_name("https://github.com/owner/repo.git/"),
            "repo"
        );
        assert_eq!(derive_repo_name(""), "repo");
    }
}
