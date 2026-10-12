//! The Changes tab: the selected repo's commit workspace — merged
//! staged/unstaged list with per-row actions and expandable patches on the
//! left, branch selector + commit box + git rail on the right. The heavy data
//! lives on the store; this view owns only ephemeral state (expanded rows,
//! fetched patches, dropdown state, the commit draft).

use crate::net::{GitAction, NetCommand};
use crate::store::Store;
use crate::theme;
use crate::ui::icon;
use devtools_core::model::GitChangedFile;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{Input, InputState, Textarea, TextareaState};
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
    /// Set when the user pressed "Generate & Commit" with an empty box: the
    /// wand's draft lands here first, then commits automatically.
    auto_commit_after_draft: bool,
    history_open: bool,
    /// Hash of the drilled-in commit (None = the list view).
    drilled_commit: Option<String>,
    /// Armed destructive drill-down action ("checkout"/"revert") — the second
    /// click confirms the arm.
    detail_armed: Option<&'static str>,
    detail_patches: std::collections::HashMap<String, String>,
    expanded_details: std::collections::HashSet<String>,
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
            auto_commit_after_draft: false,
            history_open: false,
            drilled_commit: None,
            detail_armed: None,
            detail_patches: std::collections::HashMap::new(),
            expanded_details: std::collections::HashSet::new(),
        }
    }

    /// Opens the history list, loading it once per selection.
    pub fn open_history(&mut self, cx: &mut Context<Self>) {
        let needs_load = self.store.read(cx).history.is_empty();
        self.history_open = true;
        self.drilled_commit = None;
        if needs_load
            && let Some(folder) = self.store.read(cx).selected_repo.clone() {
                let _ = self.store.read(cx).net.send_blocking(NetCommand::LoadHistory {
                    folder: std::path::PathBuf::from(folder),
                });
            }
        cx.notify();
    }

    /// Drills into one commit's detail.
    pub fn drill_commit(&mut self, hash: &str, cx: &mut Context<Self>) {
        self.drilled_commit = Some(hash.to_string());
        self.expanded_details.clear();
        if let Some(folder) = self.store.read(cx).selected_repo.clone() {
            let _ = self.store.read(cx).net.send_blocking(NetCommand::GetCommitDetails {
                folder: std::path::PathBuf::from(folder),
                hash: hash.to_string(),
            });
        }
        cx.notify();
    }

    fn toggle_detail_patch(&mut self, hash: &str, path: &str, cx: &mut Context<Self>) {
        let key = format!("{hash}|{path}");
        if self.expanded_details.contains(&key) {
            self.expanded_details.remove(&key);
        } else {
            self.expanded_details.insert(key.clone());
            if !self.detail_patches.contains_key(&key)
                && let Some(folder) = self.store.read(cx).selected_repo.clone() {
                    let _ = self.store.read(cx).net.send_blocking(NetCommand::GetCommitFilePatch {
                        folder: std::path::PathBuf::from(folder),
                        hash: hash.to_string(),
                        path: path.to_string(),
                    });
                }
        }
        cx.notify();
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // drain freshly loaded patches + the commit wand's draft
        if let Some((path, staged, patch)) = self.store.update(cx, |store, _| store.change_patch_result.take()) {
            self.patches.insert((path, staged), patch);
        }
        let mut auto_commit_text: Option<String> = None;
        if let Some(draft) = self.store.update(cx, |store, _| store.commit_message_draft.take()) {
            let draft = draft.trim().to_string();
            self.commit_message.update(cx, |state, cx| state.set_value(draft.as_str(), window, cx));
            if self.auto_commit_after_draft && !draft.is_empty() {
                self.auto_commit_after_draft = false;
                auto_commit_text = Some(draft);
            }
        }
        if let Some(message) = auto_commit_text {
            self.store.update(cx, |store, cx| store.run_git(GitAction::Commit(message), cx));
        }
        if let Some((key, patch)) = self.store.update(cx, |store, _| store.commit_file_patch.take()) {
            self.detail_patches.insert(key, patch);
        }

        let store = self.store.read(cx);
        let snapshot = store.changes.clone().unwrap_or_default();
        let (staged, unstaged) = snapshot.change_groups();
        let busy = store.git_busy;

        let left = if self.history_open {
            self.render_history(window, cx)
        } else {
            self.render_file_list(staged, unstaged, window, cx)
        };
        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .relative()
            .child(left)
            .child(self.render_rail(busy, window, cx))
    }
}

impl ChangesView {
    fn render_file_list(
        &mut self,
        staged: Vec<GitChangedFile>,
        unstaged: Vec<GitChangedFile>,
        window: &mut Window,
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
                    .text_size(px(theme::font_body()))
                    .text_color(theme::TEXT_FAINT)
                    .child(if loaded { "No changes — working tree clean" } else { "Loading…" }),
            );
            return list.into_any_element();
        }

        // order: unstaged first, then staged; headers render only when
        // their side has rows, each carrying Discard-all (+ stage/unstage all)
        if !unstaged.is_empty() {
            list = list.child(section_header(
                "Unstaged Changes",
                unstaged.len(),
                vec![
                    ("discard-all-u", "Discard", GitAction::DiscardUnstaged(
                        unstaged.iter().map(|f| f.path.clone()).collect(),
                    )),
                    ("stage-all", "Stage all", GitAction::StageAll),
                ],
                self.store.clone(),
            ));
            for file in &unstaged {
                list = list.child(self.render_row(file, false, window, cx));
            }
        }
        if !staged.is_empty() {
            list = list.child(section_header(
                "Staged Changes",
                staged.len(),
                vec![
                    ("discard-all-s", "Discard", GitAction::DiscardStaged(
                        staged.iter().map(|f| f.path.clone()).collect(),
                    )),
                    ("stage-all-clear", "Unstage all", GitAction::UnstageAll),
                ],
                self.store.clone(),
            ));
            for file in &staged {
                list = list.child(self.render_row(file, true, window, cx));
            }
        }
        list.into_any_element()
    }

    fn render_row(&mut self, file: &GitChangedFile, staged: bool, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
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
                            .text_size(px(theme::font_small() - 1.))
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
                            .text_size(px(theme::font_small()))
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
                                    .text_size(px(theme::font_small() - 1.))
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
                            .child(icon("icons/icon-discard.svg", px(theme::icon_small() - 1.), theme::TEXT_MUTED)),
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
                    .child(crate::ui::diff::render_diff(patch, theme::font_small() - 2.0, window)),
            );
        }
        row.into_any_element()
    }

    /// The history pane: day-grouped commit list with unpushed markers, and
    /// the drilled-in commit detail with expandable per-file patches.
    fn render_history(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let store = self.store.read(cx);
        let unpushed = store.unpushed.clone();
        let history = store.history.clone();
        let details = store.commit_details.clone();

        if let Some(hash) = self.drilled_commit.clone() {
            return self.render_commit_detail(hash, details, window, cx);
        }

        if history.is_empty() {
            return div()
                .id("history-list")
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::font_body()))
                .text_color(theme::TEXT_FAINT)
                .child("Loading history…")
                .into_any_element();
        }

        let mut list = div()
            .id("history-list")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_2();

        let today = chrono::Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let mut current_day: Option<chrono::NaiveDate> = None;
        for commit in &history {
            let day = commit.date.date_naive();
            if current_day != Some(day) {
                current_day = Some(day);
                let label = if day == today {
                    "Today".to_string()
                } else if day == yesterday {
                    "Yesterday".to_string()
                } else {
                    day.format("%b %e").to_string()
                };
                list = list.child(
                    div()
                        .py_1()
                        .px_1()
                        .text_size(px(theme::font_small()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::TEXT_MUTED)
                        .child(label),
                );
            }
            let unpushed_mark = unpushed
                .as_ref()
                .map(|set| set.contains(&commit.hash))
                .unwrap_or(false);
            let author_line = commit
                .author
                .clone()
                .unwrap_or_else(|| "Unknown".into());
            list = list.child(
                div()
                    .id(SharedString::from(format!("commit-{}", commit.short_hash())))
                    .cursor_pointer()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_1()
                    .py_1()
                    .rounded_md()
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        let hash = commit.hash.clone();
                        cx.listener(move |this, _, _, cx| this.drill_commit(&hash, cx))
                    })
                    .child(
                        div()
                            .rounded_full()
                            .bg(theme::CHIP)
                            .w(px(22.))
                            .h(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(theme::font_small() - 3.))
                            .text_color(theme::TEXT_MUTED)
                            .child(commit.initials()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(theme::font_small()))
                                    .text_color(theme::TEXT_DIM)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(commit.subject.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::font_small() - 2.))
                                    .text_color(theme::TEXT_FAINT)
                                    .child(format!(
                                        "{author_line} • {}",
                                        devtools_core::rel_time::format(commit.date)
                                    )),
                            ),
                    )
                    .when(unpushed_mark, |row| {
                        row.child(div().text_size(px(theme::font_small() - 1.)).text_color(theme::AMBER_BRIGHT).child("● unpushed"))
                    })
                    .child(
                        // hash chip: clicking copies the full SHA without
                        // drilling in
                        div()
                            .id(SharedString::from(format!("copy-sha-{}", commit.short_hash())))
                            .cursor_pointer()
                            .rounded_md()
                            .px_1()
                            .text_size(px(theme::font_small() - 1.))
                            .font_family(gpui::SharedString::from(theme::code_font()))
                            .text_color(theme::TEXT_FAINT)
                            .hover(|s| s.text_color(theme::ACCENT))
                            .on_click({
                                let store = self.store.clone();
                                let full = commit.hash.clone();
                                let short = commit.short_hash().to_string();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(full.clone()));
                                    store.update(cx, |s, cx| {
                                        s.notice = Some((
                                            format!("Copied {short} to clipboard"),
                                            std::time::Instant::now(),
                                        ));
                                        cx.notify();
                                    });
                                }
                            })
                            .child(commit.short_hash().to_string()),
                    ),
            );
        }
        list.into_any_element()
    }

    fn render_commit_detail(
        &mut self,
        hash: String,
        details: Option<devtools_core::model::GitCommitDetails>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(details) = details.filter(|d| d.hash.starts_with(&hash[..hash.len().min(7)])) else {
            return div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::font_body()))
                .text_color(theme::TEXT_FAINT)
                .child("Loading commit…")
                .into_any_element();
        };

        let mut list = div()
            .id("commit-detail")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_2()
            .gap_2();

        // author + time from the loaded history rows (the detail model
        // doesn't carry them)
        let history_meta = self
            .store
            .read(cx)
            .history
            .iter()
            .find(|c| c.hash == details.hash || c.hash.starts_with(&details.hash[..details.hash.len().min(7)]))
            .map(|c| {
                (
                    c.author.clone().unwrap_or_else(|| "Unknown".into()),
                    devtools_core::rel_time::format(c.date),
                )
            });

        // two-click armed destructive actions
        let armed = self.detail_armed;
        let action_button = |id: &'static str, label: &'static str, armed_now: bool| {
            let text = if armed_now { "Confirm" } else { label };
            div()
                .id(id)
                .cursor_pointer()
                .rounded_md()
                .border_1()
                .border_color(if armed_now { theme::DANGER } else { theme::HAIRLINE })
                .px_2()
                .py_0p5()
                .text_size(px(theme::font_small() - 1.))
                .text_color(if armed_now { theme::DANGER } else { theme::TEXT_DIM })
                .hover(|s| s.bg(theme::HOVER))
                .child(text.to_string())
        };

        let mut actions_row = div().flex().flex_row().gap_1();
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-checkout", "Checkout", armed == Some("checkout"))
                .on_click(move |_, _, cx| {
                    if armed == Some("checkout") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| s.run_git(GitAction::Checkout(full_hash.clone()), cx));
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("checkout");
                            cx.notify();
                        });
                    }
                })
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-revert", "Revert", armed == Some("revert"))
                .on_click(move |_, _, cx| {
                    if armed == Some("revert") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| s.run_git(GitAction::Revert(full_hash.clone()), cx));
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("revert");
                            cx.notify();
                        });
                    }
                })
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            div()
                .id("detail-copy-sha")
                .cursor_pointer()
                .rounded_md()
                .border_1()
                .border_color(theme::HAIRLINE)
                .px_2()
                .py_0p5()
                .text_size(px(theme::font_small() - 1.))
                .text_color(theme::TEXT_DIM)
                .hover(|s| s.bg(theme::HOVER))
                .on_click(move |_, _, cx| {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(full_hash.clone()));
                    store.update(cx, |s, cx| {
                        s.notice = Some((
                            format!("Copied {} to clipboard", &full_hash[..7.min(full_hash.len())]),
                            std::time::Instant::now(),
                        ));
                        cx.notify();
                    });
                })
                .child("Copy SHA")
        });

        list = list.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("detail-back")
                        .cursor_pointer()
                        .rounded_md()
                        .px_1p5()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::ACCENT)
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.drilled_commit = None;
                            this.detail_armed = None;
                            cx.notify();
                        }))
                        .child(icon("icons/icon-arrow-left.svg", px(theme::icon_small() - 1.), theme::ACCENT))
                        .child("History"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_small() - 1.))
                        .font_family(gpui::SharedString::from(theme::code_font()))
                        .text_color(theme::TEXT_FAINT)
                        .child(details.hash[..7.min(details.hash.len())].to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(format!("{} files changed", details.files.len())),
                )
                .child(actions_row),
        );

        list = list.child(
            div()
                .id("detail-files")
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    // "N files" summary row mirrors the files' totals
                    div()
                        .px_1()
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_MUTED)
                        .child(format!(
                            "+{} −{}",
                            details.files.iter().filter_map(|f| f.additions).sum::<u32>(),
                            details.files.iter().filter_map(|f| f.deletions).sum::<u32>()
                        )),
                ),
        );

        let mut files = div().flex().flex_col().gap_0p5();
        for file in &details.files {
            let key = format!("{}|{}", details.hash, file.path);
            let expanded = self.expanded_details.contains(&key);
            let mut row = div()
                .id(SharedString::from(format!("detail-file-{}", file.path)))
                .cursor_pointer()
                .flex()
                .flex_col()
                .rounded_md()
                .px_1()
                .when(expanded, |d| d.bg(theme::tint(theme::ACCENT)))
                .hover(|s| s.bg(theme::HOVER))
                .on_click({
                    let hash = details.hash.clone();
                    let path = file.path.clone();
                    cx.listener(move |this, _, _, cx| this.toggle_detail_patch(&hash, &path, cx))
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .py_0p5()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(theme::font_small()))
                                .font_family(gpui::SharedString::from(theme::code_font()))
                                .text_color(theme::TEXT_DIM)
                                .child(file.path.clone()),
                        )
                        .when_some(file.additions, |row, a| {
                            row.child(div().text_size(px(theme::font_small() - 1.)).text_color(theme::SUCCESS_BRIGHT).child(format!("+{a}")))
                        })
                        .when_some(file.deletions, |row, d| {
                            row.child(div().text_size(px(theme::font_small() - 1.)).text_color(theme::DANGER).child(format!("−{d}")))
                        }),
                );
            if expanded {
                let patch = self
                    .detail_patches
                    .get(&key)
                    .map(|p| p.as_str())
                    .unwrap_or("loading patch…");
                row = row.child(
                    div()
                        .id(SharedString::from(format!("detail-patch-{}", file.path)))
                        .ml_4()
                        .mr_1()
                        .mb_1()
                        .rounded_md()
                        .bg(theme::CARD)
                        .border_1()
                        .border_color(theme::BORDER_LIGHT)
                        .p_2()
                        .max_h(px(220.))
                        .overflow_y_scroll()
                        .child(crate::ui::diff::render_diff(patch, theme::font_small() - 2.0, window)),
                );
            }
            files = files.child(row);
        }
        list = list.child(files);

        if let Some(body) = &details.body {
            let mut card = div()
                .id("detail-body")
                .rounded_md()
                .bg(theme::CARD)
                .border_1()
                .border_color(theme::BORDER_LIGHT)
                .p_2()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_DIM)
                .child(body.clone());
            if let Some((author, when)) = history_meta {
                card = card.child(
                    div()
                        .mt_1()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .text_size(px(theme::font_small() - 2.))
                        .text_color(theme::TEXT_FAINT)
                        .child(div().child(author))
                        .child(div().child(when)),
                );
            }
            list = list.child(card);
        } else if let Some((author, when)) = history_meta {
            list = list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .text_size(px(theme::font_small() - 2.))
                    .text_color(theme::TEXT_FAINT)
                    .child(div().child(author))
                    .child(div().child(when)),
            );
        }
        list.into_any_element()
    }

    fn render_rail(&mut self, busy: bool, _window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
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
                .child(icon("icons/icon-source-branch.svg", px(theme::icon_small()), theme::TEXT_MUTED))
                .child(div().flex_1().text_size(px(theme::font_small())).child(branch.clone().unwrap_or_else(|| "—".into())))
                .child(icon("icons/icon-chevron-down.svg", px(theme::icon_small() - 2.), theme::TEXT_FAINT)),
        );

        // git rail: the .NET vertical icon list — Fetch/Pull/Push above a
        // divider, then Sync/History
        type RailAction = Box<dyn Fn(&mut gpui::App) + 'static>;
        let action_row = |icon_path: &str,
                          label: &'static str,
                          enabled: bool,
                          active: bool,
                          click: Option<RailAction>| {
            let mut row = div()
                .id(SharedString::from(format!("rail-{label}")))
                .cursor_pointer()
                .rounded_md()
                .px_2()
                .py_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_small()));
            if enabled {
                if let Some(click) = click {
                    row = row
                        .when(active, |d| d.text_color(theme::ACCENT).font_weight(FontWeight::SEMIBOLD))
                        .when(!active, |d| d.text_color(theme::TEXT_DIM))
                        .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            click(cx);
                        });
                } else {
                    row = row.text_color(theme::TEXT_DIM);
                }
            } else {
                row = row.text_color(theme::TEXT_FAINT);
            }
            let icon_color = if active {
                theme::ACCENT
            } else if enabled {
                theme::TEXT_DIM
            } else {
                theme::TEXT_FAINT
            };
            row.child(icon(icon_path, px(theme::icon_small()), icon_color))
                .child(label)
                .into_any_element()
        };
        macro_rules! rail_action {
            ($list:ident, $icon:expr, $label:expr, $enabled:expr, $action:expr) => {{
                let store = self.store.clone();
                $list = $list.child(action_row(
                    $icon,
                    $label,
                    $enabled,
                    false,
                    Some(Box::new(move |cx| {
                        store.update(cx, |s, cx| s.run_git($action.clone(), cx));
                    })),
                ));
            }};
        }
        let mut action_list = div().flex().flex_col().gap_0p5();
        rail_action!(action_list, "icons/icon-cloud-download.svg", "Fetch", !busy, GitAction::Fetch);
        rail_action!(action_list, "icons/icon-arrow-down.svg", "Pull", has_remote && !busy, GitAction::Pull);
        rail_action!(
            action_list,
            "icons/icon-arrow-up.svg",
            "Push",
            has_remote && published && !busy,
            GitAction::Push
        );
        if let Some(b) = branch.clone() {
            rail_action!(
                action_list,
                "icons/icon-arrow-up.svg",
                "Publish",
                has_remote && !published && !busy,
                GitAction::Publish { branch: b.clone() }
            );
        }
        action_list = action_list.child(div().h(px(1.)).my_1().bg(theme::HAIRLINE));
        rail_action!(action_list, "icons/icon-refresh.svg", "Sync", has_remote && !busy, GitAction::Sync);
        let history_toggle = cx.listener(|this, _, _, cx| {
            if this.history_open {
                this.history_open = false;
                this.drilled_commit = None;
            } else {
                this.open_history(cx);
            }
            cx.notify();
        });
        action_list = action_list.child(
            div()
                .id("rail-History")
                .cursor_pointer()
                .rounded_md()
                .px_2()
                .py_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .text_size(px(theme::font_small()))
                .when(self.history_open, |d| {
                    d.text_color(theme::ACCENT).font_weight(FontWeight::SEMIBOLD)
                })
                .when(!self.history_open, |d| d.text_color(theme::TEXT_DIM))
                .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                .on_click(history_toggle)
                .child(icon(
                    "icons/icon-clock.svg",
                    px(theme::icon_small()),
                    if self.history_open { theme::ACCENT } else { theme::TEXT_DIM },
                ))
                .child("History"),
        );
        rail = rail.child(action_list);
        rail = rail.child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_FAINT)
                        .child(format!("Last fetched {last_fetch}")),
                ),
        );

        // commit message + commit. "Generate & Commit":
        // an empty box generates the message with the wand first, then
        // commits; Ctrl+Enter commits from the keyboard.
        let wand_active = self.store.read(cx).wand_busy == Some(crate::net::WandKind::Commit);
        let opencode_on = self.store.read(cx).settings().opencode.enable_opencode;
        let message_empty = self.commit_message.read(cx).value().trim().is_empty();
        let generate_and_commit = message_empty && opencode_on;
        rail = rail.child(
            div()
                .flex_1()
                .min_h_0()
                .relative()
                .on_key_down({
                    let store = self.store.clone();
                    let message = self.commit_message.clone();
                    move |event, _, cx| {
                        if event.keystroke.key == "enter"
                            && event.keystroke.modifiers.control
                            && !message.read(cx).value().trim().is_empty()
                        {
                            let text = message.read(cx).value().to_string();
                            store.update(cx, |s, cx| s.run_git(GitAction::Commit(text.clone()), cx));
                        }
                    }
                })
                .child(Textarea::new(&self.commit_message).bordered(true))
                // .NET's corner wand: overlay inside the message box
                .child({
                    let store = self.store.clone();
                    div()
                        .id("wand-corner")
                        .absolute()
                        .bottom_1()
                        .right_2()
                        .cursor_pointer()
                        .rounded_md()
                        .p_1()
                        .flex()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, cx| {
                            store.update(cx, |s, cx| s.run_wand(crate::net::WandKind::Commit, cx));
                        })
                        .child(icon("icons/icon-wand.svg", px(theme::icon_small()), theme::TEXT_MUTED))
                }),
        );
        rail = rail.child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .when(wand_active, |row| {
                    row.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::ACCENT)
                            .child(icon("icons/icon-wand.svg", px(theme::icon_small() - 1.), theme::ACCENT))
                            .child("working…"),
                    )
                })
                .child(div().flex_1())
                .child({
                    let store = self.store.clone();
                    let message = self.commit_message.clone();
                    let this = cx.entity();
                    let label = if generate_and_commit { "Generate & Commit" } else { "Commit" };
                    rail_button(
                        label,
                        Some(if generate_and_commit { "icons/icon-wand.svg" } else { "icons/icon-check.svg" }),
                        !busy,
                        move |cx| {
                        let text = message.read(cx).value().to_string();
                        if text.trim().is_empty() {
                            // empty box: generate first; the drained draft
                            // lands in the textarea and then commits itself
                            let wand_busy = store.read(cx).wand_busy.is_some();
                            this.update(cx, |this, cx| {
                                this.auto_commit_after_draft = true;
                                cx.notify();
                            });
                            if !wand_busy {
                                store.update(cx, |s, cx| s.run_wand(crate::net::WandKind::Commit, cx));
                            }
                            return;
                        }
                        store.update(cx, |s, cx| s.run_git(GitAction::Commit(text.clone()), cx));
                    })
                }),
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
                                .text_size(px(theme::font_small()))
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
                                .text_size(px(theme::font_small()))
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
                    .text_size(px(theme::font_small()))
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
                    .text_size(px(theme::font_small()))
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

fn section_header(
    label: &str,
    count: usize,
    actions: Vec<(&'static str, &'static str, GitAction)>,
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
                .text_size(px(theme::font_small()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT_MUTED)
                .child(format!("{label} ({count})")),
        )
        .child(div().flex_1());
    for (id, action_label, action) in actions {
        let store = store.clone();
        header = header.child(
            div()
                .id(id)
                .cursor_pointer()
                .rounded_md()
                .px_1p5()
                .text_size(px(theme::font_small() - 1.))
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
        .text_size(px(theme::font_small()))
        .text_color(theme::ACCENT)
        .hover(|s| s.bg(theme::tint(theme::ACCENT)))
        .on_click(move |_, _, cx| {
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(glyph)
}

fn rail_button(
    label: &str,
    icon_path: Option<&'static str>,
    enabled: bool,
    on_click: impl Fn(&mut gpui::App) + 'static,
) -> gpui::AnyElement {
    let mut button = div()
        .id(SharedString::from(format!("rail-{label}")))
        .cursor_pointer()
        .rounded_md()
        .px_2()
        .py_1()
        .flex()
        .items_center()
        .gap_1()
        .text_size(px(theme::font_small()))
        .text_color(if enabled { theme::TEXT_DIM } else { theme::TEXT_FAINT })
        .border_1()
        .border_color(theme::HAIRLINE)
        .when(enabled, |d| d.hover(|s| s.bg(theme::HOVER)))
        .when(!enabled, |d| d.opacity(0.5))
        .on_click(move |_, _, cx| {
            if enabled {
                on_click(cx);
            }
        });
    if let Some(icon_path) = icon_path {
        button = button.child(icon(
            icon_path,
            px(theme::icon_small() - 1.),
            if enabled { theme::TEXT_DIM } else { theme::TEXT_FAINT },
        ));
    }
    button.child(label.to_string()).into_any_element()
}
