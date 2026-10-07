//! The Changes tab: the selected repo's commit workspace — merged
//! staged/unstaged list with per-row actions and expandable patches on the
//! left, branch selector + commit box + git rail on the right. The heavy data
//! lives on the store; this view owns only ephemeral state (expanded rows,
//! fetched patches, dropdown state, the commit draft).

use crate::net::{GitAction, NetCommand};
use crate::store::Store;
use crate::theme;
use crate::ui::icon;
use devtools_core::model::{GitChangedFile, GitChangeSnapshot};
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::Sizable as _;

pub struct ChangesView {
    pub store: Entity<Store>,
    /// (path, staged) rows currently expanded — click toggles, patch loads on
    /// first expand.
    expanded: std::collections::HashSet<(String, bool)>,
    patches: std::collections::HashMap<(String, bool), String>,
    branch_menu_open: bool,
    branch_filter: Entity<InputState>,
    new_branch_open: bool,
    new_branch_name: Entity<InputState>,
    commit_message: Entity<TextareaState>,
}

impl ChangesView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let branch_filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter branches"));
        let new_branch_name = cx.new(|cx| {
            let prefix = store.read(cx).settings().repos.branch_name_prefix.clone();
            let initial = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
            InputState::new(window, cx)
                .placeholder("branch name")
                .default_value(initial)
        });
        let commit_message = cx.new(|cx| TextareaState::new(window, cx).placeholder("Commit message"));
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            expanded: std::collections::HashSet::new(),
            patches: std::collections::HashMap::new(),
            branch_menu_open: false,
            branch_filter,
            new_branch_open: false,
            new_branch_name,
            commit_message,
        }
    }

    fn toggle_expanded(&mut self, path: &str, staged: bool, cx: &mut Context<Self>) {
        let key = (path.to_string(), staged);
        if self.expanded.contains(&key) {
            self.expanded.remove(&key);
        } else {
            self.expanded.insert(key.clone());
            if !self.patches.contains_key(&key) {
                let folder = self.store.read(cx).selected_repo.clone();
                if let Some(folder) = folder {
                    let _ = self.store.read(cx).net.send_blocking(NetCommand::GetChangePatch {
                        folder: std::path::PathBuf::from(folder),
                        path: path.to_string(),
                        staged,
                    });
                }
            }
        }
        cx.notify();
    }
}

impl Render for ChangesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // drain freshly loaded patches onto the view's cache
        if let Some((path, staged, patch)) = self.store.update(cx, |store, _| store.change_patch_result.take()) {
            self.patches.insert((path, staged), patch);
        }

        let store = self.store.read(cx);
        let snapshot = store.changes.clone().unwrap_or_default();
        let (staged, unstaged) = snapshot.change_groups();
        let busy = store.git_busy;
        drop(store);

        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .relative()
            .child(self.render_file_list(staged, unstaged, cx))
            .child(self.render_rail(busy, cx))
    }
}

impl ChangesView {
    fn render_file_list(
        &mut self,
        staged: Vec<GitChangedFile>,
        unstaged: Vec<GitChangedFile>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut list = div()
            .id("changes-list")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_2();

        if staged.is_empty() && unstaged.is_empty() {
            let loaded = self.store.read(cx).changes_loaded_for.is_some();
            list = list.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(theme::FONT_BODY))
                    .text_color(theme::TEXT_FAINT)
                    .child(if loaded { "No changes — working tree clean" } else { "Loading…" }),
            );
            return list.into_any_element();
        }

        if !staged.is_empty() {
            list = list.child(section_header(
                "Staged",
                staged.len(),
                Some(("stage-all-clear", "Unstage all", GitAction::UnstageAll)),
                self.store.clone(),
            ));
            for file in &staged {
                list = list.child(self.render_row(file, true, cx));
            }
        }
        if !unstaged.is_empty() {
            list = list.child(section_header(
                "Unstaged",
                unstaged.len(),
                Some(("stage-all", "Stage all", GitAction::StageAll)),
                self.store.clone(),
            ));
            for file in &unstaged {
                list = list.child(self.render_row(file, false, cx));
            }
        }
        list.into_any_element()
    }

    fn render_row(&mut self, file: &GitChangedFile, staged: bool, cx: &mut Context<Self>) -> gpui::AnyElement {
        let key = (file.path.clone(), staged);
        let expanded = self.expanded.contains(&key);
        let status_color = status_color(file.status_code.as_str());
        let row_id: SharedString = format!("{}-{}", if staged { "s" } else { "u" }, file.path).into();

        let mut row = div()
            .id(row_id)
            .cursor_pointer()
            .flex()
            .flex_col()
            .rounded_md()
            .px_1()
            .when(expanded, |d| d.bg(theme::tint(theme::ACCENT)))
            .hover(|s| s.bg(theme::HOVER))
            .on_click({
                let path = file.path.clone();
                cx.listener(move |this, _, _, cx| this.toggle_expanded(&path, staged, cx))
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .py_1()
                    .child(
                        div()
                            .rounded_sm()
                            .bg(theme::tint(status_color))
                            .text_color(status_color)
                            .text_size(px(theme::FONT_SMALL - 1.))
                            .px_1()
                            .w(px(18.))
                            .flex()
                            .justify_center()
                            .child(file.display_status_code().to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(theme::FONT_SMALL))
                            .font_family(gpui::SharedString::from(theme::code_font()))
                            .text_color(theme::TEXT_DIM)
                            .child(file.path.clone()),
                    )
                    .when_some(
                        file.additions.map(|a| (a, file.deletions)),
                        |row, (additions, deletions)| {
                            row.child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_1()
                                    .text_size(px(theme::FONT_SMALL - 1.))
                                    .child(div().text_color(theme::SUCCESS_BRIGHT).child(format!("+{additions}")))
                                    .when_some(deletions, |row, d| {
                                        row.child(div().text_color(theme::DANGER).child(format!("−{d}")))
                                    }),
                            )
                        },
                    )
                    .child(row_action_button(file, staged, self.store.clone()))
                    .child(
                        div()
                            .id(SharedString::from(format!("discard-{staged}-{}", file.path)))
                            .cursor_pointer()
                            .rounded_md()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::tint(theme::DANGER)))
                            .on_click({
                                let store = self.store.clone();
                                let path = file.path.clone();
                                move |_, _, cx| {
                                    store.update(cx, |s, cx| {
                                        s.run_git(
                                            GitAction::DiscardFile { path: path.clone(), is_staged: staged },
                                            cx,
                                        )
                                    });
                                }
                            })
                            .child(icon("icons/icon-discard.svg", px(theme::ICON_SMALL - 1.), theme::TEXT_MUTED)),
                    ),
            );

        if expanded {
            let patch = self
                .patches
                .get(&key)
                .map(|p| p.as_str())
                .unwrap_or("loading patch…");
            row = row.child(
                div()
                    .id(SharedString::from(format!("patch-{staged}-{}", file.path)))
                    .ml_6()
                    .mr_2()
                    .mb_1p5()
                    .rounded_md()
                    .bg(theme::CARD)
                    .border_1()
                    .border_color(theme::BORDER_LIGHT)
                    .p_2()
                    .max_h(px(220.))
                    .overflow_y_scroll()
                    .text_size(px(theme::FONT_SMALL - 1.))
                    .font_family(gpui::SharedString::from(theme::code_font()))
                    .text_color(theme::TEXT_DIM)
                    .child(patch.to_string()),
            );
        }
        row.into_any_element()
    }

    fn render_rail(&mut self, busy: bool, cx: &mut Context<Self>) -> gpui::AnyElement {
        let store = self.store.read(cx);
        let entry = store.selected_entry().cloned();
        let branch = entry.as_ref().and_then(|e| e.git_branch_name.clone());
        let has_remote = entry.as_ref().map(|e| e.git_has_remote).unwrap_or(false);
        let published = entry.as_ref().map(|e| e.git_branch_published).unwrap_or(false);
        let last_fetch = entry
            .as_ref()
            .and_then(|e| e.git_last_fetch_at)
            .map(devtools_core::rel_time::format)
            .unwrap_or_else(|| "never fetched".into());
        drop(store);

        let mut rail = div()
            .w(px(300.))
            .h_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .border_l_1()
            .border_color(theme::HAIRLINE)
            .relative();

        // branch selector button
        rail = rail.child(
            div()
                .id("branch-button")
                .cursor_pointer()
                .rounded_md()
                .border_1()
                .border_color(theme::HAIRLINE)
                .bg(theme::INPUT)
                .px_2()
                .py_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .hover(|s| s.border_color(theme::ACCENT))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.branch_menu_open = !this.branch_menu_open;
                    this.new_branch_open = false;
                    cx.notify();
                }))
                .child(icon("icons/icon-code-block.svg", px(theme::ICON_SMALL), theme::TEXT_MUTED))
                .child(div().flex_1().text_size(px(theme::FONT_SMALL)).child(branch.clone().unwrap_or_else(|| "—".into())))
                .child(icon("icons/icon-chevron-down.svg", px(theme::ICON_SMALL - 2.), theme::TEXT_FAINT)),
        );

        // git rail: fetch/pull/push + publish
        let mut buttons = div().flex().flex_row().gap_1();
        for (label, action, enabled) in [
            ("Fetch", Some(GitAction::Fetch), true),
            ("Pull", Some(GitAction::Pull), has_remote),
            ("Push", Some(GitAction::Push), has_remote && published),
            (
                "Publish",
                branch.clone().map(|b| GitAction::Publish { branch: b }),
                has_remote && !published,
            ),
        ] {
            let Some(action) = action else { continue };
            buttons = buttons.child(rail_button(
                label,
                enabled && !busy,
                {
                    let store = self.store.clone();
                    move |cx| {
                        store.update(cx, |s, cx| s.run_git(action_clone(&action), cx));
                    }
                },
            ));
        }
        rail = rail.child(buttons);
        rail = rail.child(
            div()
                .text_size(px(theme::FONT_SMALL - 1.))
                .text_color(theme::TEXT_FAINT)
                .child(format!("Last fetched {last_fetch}")),
        );

        // commit message + commit
        rail = rail.child(
            div()
                .flex_1()
                .min_h_0()
                .child(Textarea::new(&self.commit_message).bordered(true)),
        );
        rail = rail.child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    div()
                        .id("wand-button")
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::FONT_SMALL))
                        .text_color(theme::TEXT_FAINT)
                        .border_1()
                        .border_color(theme::HAIRLINE)
                        .child("✦ wand"),
                )
                .child(div().flex_1())
                .child(rail_button("Commit", !busy, {
                    let store = self.store.clone();
                    let message = self.commit_message.clone();
                    move |cx| {
                        let text = message.read(cx).value().to_string();
                        if text.trim().is_empty() {
                            return;
                        }
                        store.update(cx, |s, cx| s.run_git(GitAction::Commit(text.clone()), cx));
                    }
                })),
        );

        if self.branch_menu_open {
            rail = rail.child(self.render_branch_menu(cx));
        }
        rail.into_any_element()
    }

    fn render_branch_menu(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let filter = self.branch_filter.read(cx).value().to_string().to_lowercase();
        let current = self
            .store
            .read(cx)
            .selected_entry()
            .and_then(|e| e.git_branch_name.clone())
            .unwrap_or_default();

        if self.new_branch_open {
            return div()
                .id("branch-menu")
                .absolute()
                .top(px(-4.))
                .left(px(8.))
                .right(px(8.))
                .occlude()
                .rounded_lg()
                .border_1()
                .border_color(theme::HAIRLINE)
                .bg(theme::POPUP)
                .shadow_lg()
                .p_2()
                .flex()
                .flex_col()
                .gap_2()
                .child(Input::new(&self.new_branch_name).small())
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(
                            div()
                                .id("new-branch-cancel")
                                .cursor_pointer()
                                .rounded_md()
                                .px_2()
                                .py_1()
                                .text_size(px(theme::FONT_SMALL))
                                .text_color(theme::TEXT_DIM)
                                .border_1()
                                .border_color(theme::HAIRLINE)
                                .hover(|s| s.bg(theme::HOVER))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.new_branch_open = false;
                                    cx.notify();
                                }))
                                .child("Cancel"),
                        )
                        .child({
                            let store = self.store.clone();
                            let name_input = self.new_branch_name.clone();
                            div()
                                .id("new-branch-create")
                                .cursor_pointer()
                                .rounded_md()
                                .px_2()
                                .py_1()
                                .text_size(px(theme::FONT_SMALL))
                                .font_weight(FontWeight::SEMIBOLD)
                                .bg(theme::ACCENT)
                                .text_color(theme::ON_ACCENT)
                                .hover(|s| s.opacity(0.9))
                                .on_click(move |_, _, cx| {
                                    let name = name_input.read(cx).value().to_string();
                                    if name.trim().is_empty() {
                                        return;
                                    }
                                    store.update(cx, |s, cx| {
                                        s.run_git(
                                            GitAction::CreateBranch {
                                                name: name.trim().to_string(),
                                                start: None,
                                                checkout: true,
                                            },
                                            cx,
                                        )
                                    });
                                })
                                .child("Create")
                        }),
                )
                .into_any_element();
        }

        let mut list = div().flex().flex_col().gap_0p5();
        let mut shown = 0;
        for branch_ref in &self.store.read(cx).branches {
            if !filter.is_empty() && !branch_ref.name.to_lowercase().contains(&filter) {
                continue;
            }
            let is_current = branch_ref.name == current;
            list = list.child(
                div()
                    .id(SharedString::from(format!("branch-row-{}", branch_ref.name)))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::FONT_SMALL))
                    .when(is_current, |d| d.text_color(theme::ACCENT).font_weight(FontWeight::SEMIBOLD))
                    .when(!is_current, |d| {
                        d.text_color(if branch_ref.kind == devtools_core::model::GitBranchKind::Remote {
                            theme::TEXT_FAINT
                        } else {
                            theme::TEXT_DIM
                        })
                    })
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let store = self.store.clone();
                        let name = branch_ref.name.clone();
                        move |_, _, cx| {
                            store.update(cx, |s, cx| s.run_git(GitAction::Checkout(name.clone()), cx));
                        }
                    })
                    .child(branch_ref.name.clone()),
            );
            shown += 1;
            if shown > 30 {
                break;
            }
        }

        div()
            .id("branch-menu")
            .absolute()
            .top(px(-4.))
            .left(px(8.))
            .right(px(8.))
            .occlude()
            .rounded_lg()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::POPUP)
            .shadow_lg()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(Input::new(&self.branch_filter).small())
            .child(div().id("branch-list").flex_1().min_h_0().max_h(px(220.)).overflow_y_scroll().child(list))
            .child(
                div()
                    .id("new-branch-row")
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_1()
                    .text_size(px(theme::FONT_SMALL))
                    .text_color(theme::ACCENT)
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.new_branch_open = true;
                        cx.notify();
                    }))
                    .child("+ New branch…"),
            )
            .into_any_element()
    }
}

fn action_clone(action: &GitAction) -> GitAction {
    action.clone()
}

fn section_header(
    label: &str,
    count: usize,
    action: Option<(&'static str, &'static str, GitAction)>,
    store: Entity<Store>,
) -> impl IntoElement + use<> {
    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .py_1()
        .px_1()
        .child(
            div()
                .text_size(px(theme::FONT_SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT_MUTED)
                .child(format!("{label} ({count})")),
        )
        .child(div().flex_1());
    if let Some((id, action_label, action)) = action {
        header = header.child(
            div()
                .id(id)
                .cursor_pointer()
                .rounded_md()
                .px_1p5()
                .text_size(px(theme::FONT_SMALL - 1.))
                .text_color(theme::ACCENT)
                .hover(|s| s.bg(theme::tint(theme::ACCENT)))
                .on_click(move |_, _, cx| {
                    store.update(cx, |s, cx| s.run_git(action.clone(), cx));
                })
                .child(action_label),
        );
    }
    header
}

fn status_color(code: &str) -> gpui::Rgba {
    match code {
        "U" | "?" | "A" => theme::SUCCESS_BRIGHT,
        "D" => theme::DANGER,
        "R" => theme::BLUE_BRIGHT,
        "C" => theme::WARN,
        _ => theme::AMBER_BRIGHT,
    }
}

/// The +/− stage-toggle button per row: unstaged rows stage, staged rows
/// unstage.
fn row_action_button(file: &GitChangedFile, staged: bool, store: Entity<Store>) -> impl IntoElement + use<> {
    let action = if staged {
        GitAction::Unstage(file.path.clone())
    } else {
        GitAction::Stage(file.path.clone())
    };
    let glyph = if staged { "−" } else { "+" };
    div()
        .id(SharedString::from(format!("toggle-{staged}-{}", file.path)))
        .cursor_pointer()
        .rounded_md()
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::FONT_SMALL))
        .text_color(theme::ACCENT)
        .hover(|s| s.bg(theme::tint(theme::ACCENT)))
        .on_click(move |_, _, cx| {
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(glyph)
}

fn rail_button(
    label: &str,
    enabled: bool,
    on_click: impl Fn(&mut gpui::App) + 'static,
) -> gpui::AnyElement {
    div()
        .id(SharedString::from(format!("rail-{label}")))
        .cursor_pointer()
        .rounded_md()
        .px_2()
        .py_1()
        .text_size(px(theme::FONT_SMALL))
        .text_color(if enabled { theme::TEXT_DIM } else { theme::TEXT_FAINT })
        .border_1()
        .border_color(theme::HAIRLINE)
        .when(enabled, |d| d.hover(|s| s.bg(theme::HOVER)))
        .when(!enabled, |d| d.opacity(0.5))
        .on_click(move |_, _, cx| {
            if enabled {
                on_click(cx);
            }
        })
        .child(label.to_string())
        .into_any_element()
}
