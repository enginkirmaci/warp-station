//! The Changes tab: the selected repo's commit workspace, mirroring the .NET
//! original — three raised cards on the bar's dark surface: the merged
//! staged/unstaged file list with per-row actions and expandable patches on
//! the left, the commit-message card in the middle, and the branch selector +
//! git rail (Fetch/Pull/Push/Sync/History) on the right. The heavy data lives
//! on the store; this view owns only ephemeral state (expanded rows, fetched
//! patches, dropdown state, the commit draft).

use crate::net::{GitAction, NetCommand};
use crate::store::Store;
use crate::theme;
use crate::ui::{A11y, icon};
use devtools_core::model::GitChangedFile;
use gpui::prelude::*;
use gpui::{Context, Entity, FontWeight, SharedString, Window, div, px};
use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{InputState, Textarea, TextareaState};
use std::cell::RefCell;
use std::rc::Rc;
use warpui_kit_picker::{
    FilteredPicker, OverlayAnchor, PanelChrome, PickerBody, PickerData, PickerIcons, PickerItem,
    PickerRow, PickerSpec, PickerTarget, Selection, TrailingDelete, TriggerScale,
};

/// One history row's display strings, precomputed when the history (or the
/// day) changes so a render never re-derives them per row.
struct HistoryRow {
    /// Some(..) on the first commit of a day: the group header label.
    day_label: Option<String>,
    initials: String,
    author: String,
    rel: String,
    short_hash: String,
    subject: String,
    hash: String,
}

/// The staged/unstaged split produced by `GitChangeSnapshot::change_groups`.
type GroupedFiles = (Vec<GitChangedFile>, Vec<GitChangedFile>);

pub struct ChangesView {
    pub store: Entity<Store>,
    /// (path, staged) rows currently expanded — click toggles, patch loads on
    /// first expand.
    expanded: std::collections::HashSet<(String, bool)>,
    patches: std::collections::HashMap<(String, bool), String>,
    /// The branch selector: the shared FilteredPicker in branch mode.
    branch_picker: Entity<FilteredPicker>,
    /// The branch picker's ephemeral state (armed delete, open form); shared
    /// with the picker's closures.
    branch_ui: Rc<RefCell<BranchUi>>,
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
    /// The repo the view's local state belongs to; a mismatch resets it all.
    loaded_for: Option<String>,
    /// Staged/unstaged groups, rebuilt only when the store's change snapshot
    /// generation moves (any store notify otherwise re-cloned everything).
    groups_cache: Option<(u64, Rc<GroupedFiles>)>,
    /// Precomputed history row labels, keyed by history generation + today.
    history_rows: Vec<HistoryRow>,
    history_cache_key: (u64, chrono::NaiveDate),
}

impl ChangesView {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let new_branch_name = cx.new(|cx| InputState::new(window, cx).placeholder("branch name"));
        let branch_ui: Rc<RefCell<BranchUi>> = Rc::new(RefCell::new(BranchUi::default()));
        let branch_picker = build_branch_picker(
            &store,
            branch_ui.clone(),
            new_branch_name.clone(),
            window,
            cx,
        );
        // the picker's open popup is appended by render_rail as the rail's
        // last child, so picker-internal changes (filter typing, arming,
        // dismissal) must repaint this view too
        cx.observe(&branch_picker, |_, _, cx| cx.notify()).detach();
        let commit_message =
            cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe your changes…"));
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            expanded: std::collections::HashSet::new(),
            patches: std::collections::HashMap::new(),
            branch_picker,
            branch_ui,
            commit_message,
            auto_commit_after_draft: false,
            history_open: false,
            drilled_commit: None,
            detail_armed: None,
            detail_patches: std::collections::HashMap::new(),
            expanded_details: std::collections::HashSet::new(),
            loaded_for: None,
            groups_cache: None,
            history_rows: Vec::new(),
            history_cache_key: (0, chrono::NaiveDate::MIN),
        }
    }

    /// One-time reset when the selected repo changed: the bar must never show
    /// the previous repo's draft, expansion, menu or drill-down state.
    fn sync_repo_reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let repo = self.store.read(cx).selected_repo.clone();
        if self.loaded_for == repo {
            return;
        }
        self.loaded_for = repo;
        self.expanded.clear();
        self.patches.clear();
        self.branch_picker.update(cx, FilteredPicker::dismiss);
        {
            // the branch popup must never show the previous repo's list,
            // filter or form
            let mut ui = self.branch_ui.borrow_mut();
            ui.delete_armed = None;
            ui.new_branch_open = false;
        }
        self.branch_picker
            .update(cx, |picker, cx| picker.clear_filter(window, cx));
        self.commit_message
            .update(cx, |f, cx| f.set_value("", window, cx));
        self.auto_commit_after_draft = false;
        self.history_open = false;
        self.drilled_commit = None;
        self.detail_armed = None;
        self.detail_patches.clear();
        self.expanded_details.clear();
    }

    /// Closes the branch picker from an outside click or Escape. Returns
    /// whether it was actually open so the bar can stop propagating.
    pub fn dismiss_branch_menu(&mut self, cx: &mut Context<Self>) -> bool {
        self.branch_picker.update(cx, FilteredPicker::dismiss)
    }

    /// Opens the history list, loading it once per selection.
    pub fn open_history(&mut self, cx: &mut Context<Self>) {
        let needs_load = self.store.read(cx).history().is_empty();
        self.history_open = true;
        self.drilled_commit = None;
        if needs_load && let Some(folder) = self.store.read(cx).selected_repo.clone() {
            let _ = self
                .store
                .read(cx)
                .net
                .send_blocking(NetCommand::LoadHistory {
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
            let _ = self
                .store
                .read(cx)
                .net
                .send_blocking(NetCommand::GetCommitDetails {
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
                && let Some(folder) = self.store.read(cx).selected_repo.clone()
            {
                let _ = self
                    .store
                    .read(cx)
                    .net
                    .send_blocking(NetCommand::GetCommitFilePatch {
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
                    let _ = self
                        .store
                        .read(cx)
                        .net
                        .send_blocking(NetCommand::GetChangePatch {
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
        self.sync_repo_reset(window, cx);
        // drain freshly loaded patches + the commit wand's draft; patches are
        // \r-stripped once here so the diff renderer never rewrites the text
        // per render
        if let Some((path, staged, patch)) = self
            .store
            .update(cx, |store, _| store.change_patch_result.take())
        {
            self.patches.insert((path, staged), patch.replace('\r', ""));
        }
        // a settled commit empties the message box (before the draft drain so
        // a wand draft landing in the same pass survives)
        if self.store.update(cx, |store, _| {
            std::mem::take(&mut store.commit_message_clear)
        }) {
            self.commit_message
                .update(cx, |state, cx| state.set_value("", window, cx));
        }
        let mut auto_commit_text: Option<String> = None;
        if let Some(draft) = self
            .store
            .update(cx, |store, _| store.commit_message_draft.take())
        {
            let draft = draft.trim().to_string();
            self.commit_message
                .update(cx, |state, cx| state.set_value(draft.as_str(), window, cx));
            if self.auto_commit_after_draft && !draft.is_empty() {
                self.auto_commit_after_draft = false;
                auto_commit_text = Some(draft);
            }
        }
        if let Some(message) = auto_commit_text {
            self.store.update(cx, |store, cx| {
                store.run_git(GitAction::Commit(message), cx)
            });
        }
        if let Some((key, patch)) = self
            .store
            .update(cx, |store, _| store.commit_file_patch.take())
        {
            self.detail_patches.insert(key, patch.replace('\r', ""));
        }

        let (generation, busy) = {
            let store = self.store.read(cx);
            (store.changes_generation(), store.git_busy)
        };
        if self
            .groups_cache
            .as_ref()
            .is_none_or(|(g, _)| *g != generation)
        {
            let groups = self
                .store
                .read(cx)
                .changes()
                .cloned()
                .unwrap_or_default()
                .change_groups();
            self.groups_cache = Some((generation, Rc::new(groups)));
        }
        let groups = self.groups_cache.as_ref().expect("groups cached").1.clone();

        // the .NET Changes tab: three raised cards — file list, commit box,
        // git rail — on the bar's dark surface
        let left = div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .rounded_lg()
            .bg(theme::STRONG_BACKGROUND)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(if self.history_open {
                self.render_history(window, cx)
            } else {
                self.render_file_list(&groups, window, cx)
            });
        div()
            .size_full()
            .flex()
            .flex_row()
            .gap_2()
            .p_2()
            .min_h_0()
            .child(left)
            .child(self.render_commit_card(busy, window, cx))
            .child(self.render_rail(busy, window, cx))
    }
}

impl ChangesView {
    fn render_file_list(
        &mut self,
        groups: &Rc<GroupedFiles>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let (staged, unstaged) = (&groups.0, &groups.1);
        let mut list = div()
            .id("changes-list")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .px_2()
            .py_1();

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
                    .child(if loaded {
                        "No changes — working tree clean"
                    } else {
                        "Loading…"
                    }),
            );
            return list.into_any_element();
        }

        // order: unstaged first, then staged; headers render only when
        // their side has rows, each carrying Discard-all (+ stage/unstage all)
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        if !unstaged.is_empty() {
            list = list.child(section_header(
                "Unstaged Changes",
                unstaged.len(),
                false,
                show_tooltips,
                vec![
                    (
                        "discard-all-u",
                        "Discard",
                        GitAction::DiscardUnstaged(
                            unstaged.iter().map(|f| f.path.clone()).collect(),
                        ),
                    ),
                    ("stage-all", "Stage All", GitAction::StageAll),
                ],
                self.store.clone(),
            ));
            for file in unstaged {
                list = list.child(self.render_row(file, false, window, cx));
            }
        }
        if !staged.is_empty() {
            list = list.child(section_header(
                "Staged Changes",
                staged.len(),
                true,
                show_tooltips,
                vec![
                    (
                        "discard-all-s",
                        "Discard",
                        GitAction::DiscardStaged(staged.iter().map(|f| f.path.clone()).collect()),
                    ),
                    ("stage-all-clear", "Unstage All", GitAction::UnstageAll),
                ],
                self.store.clone(),
            ));
            for file in staged {
                list = list.child(self.render_row(file, true, window, cx));
            }
        }
        list.into_any_element()
    }

    fn render_row(
        &mut self,
        file: &GitChangedFile,
        staged: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = (file.path.clone(), staged);
        let expanded = self.expanded.contains(&key);
        let status_color = status_color(file.status_code.as_str());
        let row_id: SharedString =
            format!("{}-{}", if staged { "s" } else { "u" }, file.path).into();

        let mut row = div()
            .id(row_id)
            .a11y_tree_item(file.path.clone(), Some(expanded))
            .cursor_pointer()
            .group("file-row")
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
                        // expander chevron: right when folded, down when open
                        div().w(px(12.)).flex().justify_center().child(icon(
                            if expanded {
                                "icons/icon-chevron-down.svg"
                            } else {
                                "icons/icon-chevron-right.svg"
                            },
                            px(theme::icon_small() - 2.),
                            theme::TEXT_FAINT,
                        )),
                    )
                    .child(
                        div()
                            .rounded_md()
                            .bg(theme::tint(status_color))
                            .text_color(status_color)
                            .text_size(px(theme::font_small() - 1.))
                            .px_1()
                            .w(px(20.))
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
                    // hover-revealed stage toggle + discard, like the .NET rows
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_0p5()
                            .opacity(0.)
                            .group_hover("file-row", |s| s.opacity(1.))
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "discard-{staged}-{}",
                                        file.path
                                    )))
                                    .a11y_button(format!("Discard {}", file.path))
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
                                            cx.stop_propagation();
                                            store.update(cx, |s, cx| {
                                                s.run_git(
                                                    GitAction::DiscardFile {
                                                        path: path.clone(),
                                                        is_staged: staged,
                                                    },
                                                    cx,
                                                )
                                            });
                                        }
                                    })
                                    .child(icon(
                                        "icons/icon-undo.svg",
                                        px(theme::icon_small() - 1.),
                                        theme::TEXT_MUTED,
                                    )),
                            )
                            .child(row_action_button(file, staged, self.store.clone())),
                    )
                    .when_some(
                        file.additions.map(|a| (a, file.deletions)),
                        |row, (additions, deletions)| {
                            row.child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_1()
                                    .child(count_pill(
                                        format!("+{additions}"),
                                        theme::SUCCESS_BRIGHT,
                                    ))
                                    .when_some(deletions, |row, d| {
                                        row.child(count_pill(format!("−{d}"), theme::DANGER))
                                    }),
                            )
                        },
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
                    .py_2()
                    .max_h(px(220.))
                    .overflow_hidden()
                    .overflow_y_scroll()
                    .child(warpui_kit_diff::render_diff(
                        patch,
                        None,
                        theme::font_small() - 2.0,
                        window,
                        cx,
                    )),
            );
        }
        row.into_any_element()
    }

    /// The history pane: day-grouped commit list with unpushed markers, and
    /// the drilled-in commit detail with expandable per-file patches. Row
    /// labels are precomputed (see [`HistoryRow`]); only the element tree is
    /// rebuilt per render.
    fn render_history(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        if let Some(hash) = self.drilled_commit.clone() {
            let details = self.store.read(cx).commit_details.clone();
            return self.render_commit_detail(hash, details, window, cx);
        }

        let (unpushed, generation) = {
            let store = self.store.read(cx);
            (store.unpushed.clone(), store.history_generation())
        };
        let today = chrono::Local::now().date_naive();
        if self.history_cache_key != (generation, today) {
            let yesterday = today - chrono::Duration::days(1);
            let mut current_day: Option<chrono::NaiveDate> = None;
            self.history_rows = self
                .store
                .read(cx)
                .history()
                .iter()
                .map(|commit| {
                    let day = commit.date.date_naive();
                    let day_label = if current_day != Some(day) {
                        current_day = Some(day);
                        Some(if day == today {
                            "Today".to_string()
                        } else if day == yesterday {
                            "Yesterday".to_string()
                        } else {
                            day.format("%b %e").to_string()
                        })
                    } else {
                        None
                    };
                    HistoryRow {
                        day_label,
                        initials: commit.initials(),
                        author: commit.author.clone().unwrap_or_else(|| "Unknown".into()),
                        rel: devtools_core::rel_time::format(commit.date),
                        short_hash: commit.short_hash().to_string(),
                        subject: commit.subject.clone(),
                        hash: commit.hash.clone(),
                    }
                })
                .collect();
            self.history_cache_key = (generation, today);
        }
        if self.history_rows.is_empty() {
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

        for row in &self.history_rows {
            if let Some(label) = &row.day_label {
                list = list.child(
                    div()
                        .py_1()
                        .px_1()
                        .text_size(px(theme::font_small()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::TEXT_MUTED)
                        .child(label.clone()),
                );
            }
            let unpushed_mark = unpushed.as_ref().is_some_and(|set| set.contains(&row.hash));
            list = list.child(
                div()
                    .id(SharedString::from(format!("commit-{}", row.short_hash)))
                    .a11y_button(format!("{} — {}, {}", row.subject, row.author, row.rel))
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
                        let hash = row.hash.clone();
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
                            .child(row.initials.clone()),
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
                                    .child(row.subject.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(theme::font_small() - 2.))
                                    .text_color(theme::TEXT_FAINT)
                                    .child(format!("{} • {}", row.author, row.rel)),
                            ),
                    )
                    .when(unpushed_mark, |row| {
                        row.child(
                            div()
                                .text_size(px(theme::font_small() - 1.))
                                .text_color(theme::AMBER_BRIGHT)
                                .child("● unpushed"),
                        )
                    })
                    .child(
                        // hash chip: clicking copies the full SHA without
                        // drilling in
                        div()
                            .id(SharedString::from(format!("copy-sha-{}", row.short_hash)))
                            .a11y_button(format!("Copy SHA {}", row.short_hash))
                            .cursor_pointer()
                            .rounded_md()
                            .px_1()
                            .text_size(px(theme::font_small() - 1.))
                            .font_family(gpui::SharedString::from(theme::code_font()))
                            .text_color(theme::TEXT_FAINT)
                            .hover(|s| s.text_color(theme::ACCENT))
                            .on_click({
                                let store = self.store.clone();
                                let full = row.hash.clone();
                                let short = row.short_hash.clone();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                        full.clone(),
                                    ));
                                    store.update(cx, |s, cx| {
                                        s.notify_success(format!("Copied {short} to clipboard"));
                                        cx.notify();
                                    });
                                }
                            })
                            .child(row.short_hash.clone()),
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
        let Some(details) = details.filter(|d| d.hash.starts_with(&hash[..hash.len().min(7)]))
        else {
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
            .history()
            .iter()
            .find(|c| {
                c.hash == details.hash
                    || c.hash
                        .starts_with(&details.hash[..details.hash.len().min(7)])
            })
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
                .a11y_button(text)
                .cursor_pointer()
                .rounded_md()
                .border_1()
                .border_color(if armed_now {
                    theme::DANGER
                } else {
                    theme::HAIRLINE
                })
                .px_2()
                .py_0p5()
                .text_size(px(theme::font_small() - 1.))
                .text_color(if armed_now {
                    theme::DANGER
                } else {
                    theme::TEXT_DIM
                })
                .hover(|s| s.bg(theme::HOVER))
                .child(text.to_string())
        };

        let mut actions_row = div().flex().flex_row().gap_1();
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-checkout", "Checkout", armed == Some("checkout")).on_click(
                move |_, _, cx| {
                    if armed == Some("checkout") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(GitAction::Checkout(full_hash.clone()), cx)
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("checkout");
                            cx.notify();
                        });
                    }
                },
            )
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            let this = cx.entity();
            action_button("detail-revert", "Revert", armed == Some("revert")).on_click(
                move |_, _, cx| {
                    if armed == Some("revert") {
                        this.update(cx, |this, _| this.detail_armed = None);
                        store.update(cx, |s, cx| {
                            s.run_git(GitAction::Revert(full_hash.clone()), cx)
                        });
                    } else {
                        this.update(cx, |this, cx| {
                            this.detail_armed = Some("revert");
                            cx.notify();
                        });
                    }
                },
            )
        });
        actions_row = actions_row.child({
            let store = self.store.clone();
            let full_hash = details.hash.clone();
            div()
                .id("detail-copy-sha")
                .a11y_button("Copy SHA")
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
                        s.notify_success(format!(
                            "Copied {} to clipboard",
                            &full_hash[..7.min(full_hash.len())]
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
                        .a11y_button("Back to history")
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
                        .child(icon(
                            "icons/icon-arrow-left.svg",
                            px(theme::icon_small() - 1.),
                            theme::ACCENT,
                        ))
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
            div().id("detail-files").flex().flex_col().gap_0p5().child(
                // "N files" summary row mirrors the files' totals
                div()
                    .px_1()
                    .text_size(px(theme::font_small() - 1.))
                    .text_color(theme::TEXT_MUTED)
                    .child(format!(
                        "+{} −{}",
                        details
                            .files
                            .iter()
                            .filter_map(|f| f.additions)
                            .sum::<u32>(),
                        details
                            .files
                            .iter()
                            .filter_map(|f| f.deletions)
                            .sum::<u32>()
                    )),
            ),
        );

        let mut files = div().flex().flex_col().gap_0p5();
        for file in &details.files {
            let key = format!("{}|{}", details.hash, file.path);
            let expanded = self.expanded_details.contains(&key);
            let mut row = div()
                .id(SharedString::from(format!("detail-file-{}", file.path)))
                .a11y_tree_item(file.path.clone(), Some(expanded))
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
                            row.child(
                                div()
                                    .text_size(px(theme::font_small() - 1.))
                                    .text_color(theme::SUCCESS_BRIGHT)
                                    .child(format!("+{a}")),
                            )
                        })
                        .when_some(file.deletions, |row, d| {
                            row.child(
                                div()
                                    .text_size(px(theme::font_small() - 1.))
                                    .text_color(theme::DANGER)
                                    .child(format!("−{d}")),
                            )
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
                        .py_2()
                        .max_h(px(220.))
                        .overflow_hidden()
                        .overflow_y_scroll()
                        .child(warpui_kit_diff::render_diff(
                            patch,
                            None,
                            theme::font_small() - 2.0,
                            window,
                            cx,
                        )),
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

    /// The middle card: "Commit message" title, the message textarea with
    /// the wand in its corner, and the outlined commit button. An empty box
    /// turns the button into "Generate & Commit": the wand drafts the
    /// message first, then the commit fires itself.
    fn render_commit_card(
        &mut self,
        busy: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let wand_active = {
            let state = self.store.read(cx);
            state.wand_busy == Some(crate::net::WandKind::Commit)
                && state.wand_repo.as_deref() == state.selected_repo.as_deref()
        };
        let wand_on = self.store.read(cx).wand_available();
        let message_empty = self.commit_message.read(cx).value().trim().is_empty();
        let generate_and_commit = message_empty && wand_on;

        let mut card = div()
            .w(px(320.))
            .h_full()
            .min_h_0()
            .rounded_lg()
            .bg(theme::STRONG_BACKGROUND)
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(theme::font_small()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::TEXT)
                            .child("Commit message"),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(theme::font_small() - 2.))
                            .text_color(theme::TEXT_FAINT)
                            .child(if generate_and_commit {
                                "empty = auto-generate"
                            } else {
                                "Ctrl+Enter to commit"
                            }),
                    ),
            )
            .child(
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
                                store.update(cx, |s, cx| {
                                    s.run_git(GitAction::Commit(text.clone()), cx)
                                });
                            }
                        }
                    })
                    .child(Textarea::new(&self.commit_message).bordered(true).h_full())
                    // .NET's corner wand: overlay inside the message box
                    .when(wand_on, |box_area| {
                        let store = self.store.clone();
                        box_area.child(
                            div()
                                .id("wand-corner")
                                .a11y_button("Generate commit message")
                                .absolute()
                                .bottom_1()
                                .right_2()
                                .cursor_pointer()
                                .rounded_md()
                                .p_1()
                                .flex()
                                .hover(|s| s.bg(theme::HOVER))
                                .on_click(move |_, _, cx| {
                                    store.update(cx, |s, cx| {
                                        s.run_wand(crate::net::WandKind::Commit, cx)
                                    });
                                })
                                .child(icon(
                                    "icons/icon-wand.svg",
                                    px(theme::icon_small()),
                                    theme::TEXT_MUTED,
                                )),
                        )
                    }),
            );

        let mut button_row = div().flex().flex_row().items_center().gap_2();
        button_row = button_row.child({
            let store = self.store.clone();
            let message = self.commit_message.clone();
            let this = cx.entity();
            let label = if generate_and_commit {
                "Generate & Commit"
            } else {
                "Commit"
            };
            let can_commit = !busy && (!message_empty || wand_on);
            let color = if can_commit {
                theme::ON_ACCENT
            } else {
                theme::TEXT_FAINT
            };
            div()
                .id("commit-button")
                .a11y_button(label)
                .cursor_pointer()
                .w_full()
                .h(px(34.))
                .rounded_md()
                .flex()
                .items_center()
                .justify_center()
                .gap_1p5()
                .text_color(color)
                .when(can_commit, |b| {
                    b.bg(theme::ACCENT).hover(|s| s.opacity(0.9))
                })
                .when(!can_commit, |b| b.bg(theme::CHIP))
                .on_click(move |_, _, cx| {
                    if busy {
                        return;
                    }
                    let text = message.read(cx).value().to_string();
                    if text.trim().is_empty() {
                        if !wand_on {
                            return;
                        }
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
                .child(icon(
                    if generate_and_commit {
                        "icons/icon-wand.svg"
                    } else {
                        "icons/icon-check.svg"
                    },
                    px(theme::icon_small()),
                    color,
                ))
                .child(
                    div()
                        .text_size(px(theme::font_small()))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color)
                        .child(label),
                )
        });
        if wand_active {
            button_row = button_row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(theme::font_small()))
                    .text_color(theme::ACCENT)
                    .child(icon(
                        "icons/icon-wand.svg",
                        px(theme::icon_small() - 1.),
                        theme::ACCENT,
                    ))
                    .child("working…"),
            );
        }
        card = card.child(button_row);
        card.into_any_element()
    }

    fn render_rail(
        &mut self,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let store = self.store.read(cx);
        let branch = store
            .selected_entry()
            .and_then(|e| e.git_branch_name.clone());
        let has_remote = store
            .selected_entry()
            .map(|e| e.git_has_remote)
            .unwrap_or(false);
        let published = store
            .selected_entry()
            .map(|e| e.git_branch_published)
            .unwrap_or(false);
        let to_push = store
            .selected_entry()
            .map(|e| e.git_to_push_count)
            .unwrap_or(0);
        let to_pull = store
            .selected_entry()
            .map(|e| e.git_to_pull_count)
            .unwrap_or(0);

        let mut rail = div()
            .w(px(280.))
            .h_full()
            .min_h_0()
            .rounded_lg()
            .bg(theme::STRONG_BACKGROUND)
            .p_2()
            .flex()
            .flex_col()
            .relative();

        // the branch selector is the shared FilteredPicker in branch mode;
        // its open popup defers painting over the rail's later children and
        // dismissal flows through the window's outside-click/Escape chain
        rail = rail.child(self.branch_picker.clone());
        rail = rail.child(divider());

        // git rail: the .NET vertical icon list — Fetch/Pull/Push above a
        // divider, then Sync/History
        type RailAction = Box<dyn Fn(&mut gpui::App) + 'static>;
        let action_row = |icon_path: &str,
                          label: &'static str,
                          enabled: bool,
                          active: bool,
                          badge: Option<(u32, gpui::Rgba)>,
                          click: Option<RailAction>| {
            let mut row = div()
                .id(SharedString::from(format!("rail-{label}")))
                .debug_selector(move || format!("rail-{label}"))
                .a11y_button(label)
                .cursor_pointer()
                .rounded_md()
                .px_2()
                .py_1p5()
                .flex()
                .flex_row()
                .items_center()
                .gap_2p5()
                .text_size(px(theme::font_body()));
            if enabled {
                if let Some(click) = click {
                    row = row
                        .when(active, |d| {
                            d.text_color(theme::ACCENT)
                                .font_weight(FontWeight::SEMIBOLD)
                        })
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
                // fills the row so a pending-work pill pins to the right edge
                // instead of hugging the label
                .child(div().flex_1().min_w_0().child(label))
                .when_some(badge.filter(|_| enabled), |row, (count, color)| {
                    // the table row's pending-work pill, on the action that
                    // resolves it
                    let selector = format!("rail-badge-{label}");
                    row.child(
                        div()
                            .debug_selector(move || selector.clone())
                            .rounded_md()
                            .bg(theme::tint(color))
                            .px_1p5()
                            .py_0p5()
                            .text_size(px(theme::font_small() - 1.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(color)
                            .child(format!("{count}")),
                    )
                })
                .into_any_element()
        };
        macro_rules! rail_action {
            ($list:ident, $icon:expr, $label:expr, $enabled:expr, $badge:expr, $action:expr) => {{
                let store = self.store.clone();
                $list = $list.child(action_row(
                    $icon,
                    $label,
                    $enabled,
                    false,
                    $badge,
                    Some(Box::new(move |cx| {
                        store.update(cx, |s, cx| s.run_git($action.clone(), cx));
                    })),
                ));
            }};
        }
        let mut action_list = div().flex().flex_col().gap_0p5();
        rail_action!(
            action_list,
            "icons/icon-cloud-download.svg",
            "Fetch",
            !busy,
            None,
            GitAction::Fetch
        );
        rail_action!(
            action_list,
            "icons/icon-arrow-down.svg",
            "Pull",
            has_remote && !busy,
            (to_pull > 0).then_some((to_pull, theme::BLUE_BRIGHT)),
            GitAction::Pull
        );
        rail_action!(
            action_list,
            "icons/icon-arrow-up.svg",
            "Push",
            has_remote && published && !busy,
            (to_push > 0).then_some((to_push, theme::SUCCESS_BRIGHT)),
            GitAction::Push
        );
        if let Some(b) = branch.clone() {
            rail_action!(
                action_list,
                "icons/icon-arrow-up.svg",
                "Publish",
                has_remote && !published && !busy,
                None,
                GitAction::Publish { branch: b.clone() }
            );
        }
        action_list = action_list.child(divider());
        rail_action!(
            action_list,
            "icons/icon-refresh.svg",
            "Sync",
            has_remote && !busy,
            None,
            GitAction::Sync
        );
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
                .a11y_toggle("History", self.history_open)
                .cursor_pointer()
                .rounded_md()
                .px_2()
                .py_1p5()
                .flex()
                .flex_row()
                .items_center()
                .gap_2p5()
                .text_size(px(theme::font_body()))
                .when(self.history_open, |d| {
                    d.text_color(theme::ACCENT)
                        .font_weight(FontWeight::SEMIBOLD)
                })
                .when(!self.history_open, |d| d.text_color(theme::TEXT_DIM))
                .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                .on_click(history_toggle)
                .child(icon(
                    "icons/icon-clock.svg",
                    px(theme::icon_small()),
                    if self.history_open {
                        theme::ACCENT
                    } else {
                        theme::TEXT_DIM
                    },
                ))
                .child("History"),
        );
        rail = rail.child(action_list);

        // the open branch popup rides LAST so later-sibling paint order puts
        // it over the rail without a deferred wrapper
        if let Some(popup) = self
            .branch_picker
            .update(cx, |picker, cx| picker.overlay_popup(window, cx))
        {
            rail = rail.child(popup);
        }

        rail.into_any_element()
    }
}

fn section_header(
    label: &str,
    count: usize,
    with_check: bool,
    show_tooltips: bool,
    actions: Vec<(&'static str, &'static str, GitAction)>,
    store: Entity<Store>,
) -> impl IntoElement + use<> {
    let mut header = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .py_1p5()
        .px_1()
        .when(with_check, |h| {
            h.child(icon(
                "icons/icon-check.svg",
                px(theme::icon_small() - 1.),
                theme::SUCCESS_DEEP,
            ))
        })
        .child(
            div()
                .text_size(px(theme::font_body()))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme::TEXT)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(px(theme::font_small()))
                .text_color(theme::TEXT_FAINT)
                .child(format!("({count})")),
        )
        .child(div().flex_1());
    for (id, action_label, action) in actions {
        let store = store.clone();
        let stage_action = action_label.starts_with("Stage") || action_label.starts_with("Unstage");
        // the .NET header pair as bare glyphs: undo arrow for discard,
        // plus/minus for (un)staging — the label survives as the tooltip
        let glyph = if stage_action {
            if action_label.starts_with("Unstage") {
                "icons/icon-minus.svg"
            } else {
                "icons/icon-plus.svg"
            }
        } else {
            "icons/icon-undo.svg"
        };
        let color = if stage_action {
            theme::ACCENT
        } else {
            theme::TEXT_MUTED
        };
        let button = div()
            .id(id)
            .a11y_button(action_label)
            .cursor_pointer()
            .rounded_md()
            .p_1()
            .flex()
            .items_center()
            .text_color(color)
            .hover(move |s| {
                if stage_action {
                    s.bg(theme::tint(theme::ACCENT))
                } else {
                    s.bg(theme::tint(theme::DANGER)).text_color(theme::DANGER)
                }
            })
            .on_click(move |_, _, cx| {
                store.update(cx, |s, cx| s.run_git(action.clone(), cx));
            })
            .child(icon(glyph, px(theme::icon_small()), color));
        header = header.child(crate::ui::with_tooltip(button, show_tooltips, action_label));
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
fn row_action_button(
    file: &GitChangedFile,
    staged: bool,
    store: Entity<Store>,
) -> impl IntoElement + use<> {
    let action = if staged {
        GitAction::Unstage(file.path.clone())
    } else {
        GitAction::Stage(file.path.clone())
    };
    let glyph = if staged {
        "icons/icon-minus.svg"
    } else {
        "icons/icon-plus.svg"
    };
    div()
        .id(SharedString::from(format!("toggle-{staged}-{}", file.path)))
        .a11y_button(if staged {
            format!("Unstage {}", file.path)
        } else {
            format!("Stage {}", file.path)
        })
        .cursor_pointer()
        .rounded_md()
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme::ACCENT)
        .hover(|s| s.bg(theme::tint(theme::ACCENT)))
        .on_click(move |_, _, cx| {
            cx.stop_propagation();
            store.update(cx, |s, cx| s.run_git(action.clone(), cx));
        })
        .child(icon(glyph, px(theme::icon_small() - 2.), theme::ACCENT))
}

/// The rounded +N/−N diff pill: bright count on its tinted ground.
fn count_pill(text: String, color: gpui::Rgba) -> impl IntoElement + use<> {
    div()
        .rounded_full()
        .bg(theme::tint(color))
        .px_1p5()
        .text_size(px(theme::font_small() - 1.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .child(text)
}

/// The hairline separator between rail sections.
fn divider() -> impl IntoElement + use<> {
    div().h(px(1.)).my_1p5().bg(theme::HAIRLINE)
}

// ---- the branch picker ------------------------------------------------------

/// The app's icon paths for the shared picker shell.
const PICKER_ICONS: PickerIcons = PickerIcons {
    check: "icons/icon-check.svg",
    chevron_down: "icons/icon-chevron-down.svg",
    trash: "icons/icon-trash.svg",
    refresh: "icons/icon-refresh.svg",
};

/// The branch picker's ephemeral state: the armed delete, whether the
/// new-branch form replaced the list, and the repo the popup was loaded for
/// (a mismatch resets it).
#[derive(Default)]
struct BranchUi {
    delete_armed: Option<String>,
    new_branch_open: bool,
}

/// The branch selector: a pick checks the branch out; locals carry the delete
/// trash and the popup carries the new-branch form.
fn build_branch_picker(
    store: &Entity<Store>,
    branch_ui: Rc<RefCell<BranchUi>>,
    new_branch_name: Entity<InputState>,
    window: &mut Window,
    cx: &mut Context<ChangesView>,
) -> Entity<FilteredPicker> {
    let spec = PickerSpec {
        key: "Branch",
        trigger_label: "Branch",
        filter_placeholder: "Filter branches",
        icons: PICKER_ICONS,
        trigger_icon: Some("icons/icon-source-branch.svg"),
        provider_badge: false,
        refresh: false,
        refresh_label: "",
        stop_trigger_mousedown: true,
        visible_rows: 8,
        selected_accent: true,
        overlay: true,
        anchor: OverlayAnchor::Rail,
        trigger_scale: TriggerScale::Compact,
        trigger_rounded_lg: false,
        trigger_min_h: None,
        panel_chrome: PanelChrome::Menus,
        trigger_id: Some("branch-button"),
        panel_id: Some("branch-menu"),
        filter_selector: Some("branch-filter"),
        list_id: Some("branch-list"),
    };
    let pick_store = store.clone();
    let branch_picker = cx.new(|cx| {
        FilteredPicker::new(
            spec,
            PickerTarget::Callback(Box::new(move |value, _window, cx| {
                pick_store.update(cx, |s, cx| {
                    s.run_git(GitAction::Checkout(value.to_string()), cx)
                });
            })),
            Box::new(|_, _, _| unreachable!("the real source lands right after construction")),
            window,
            cx,
        )
    });
    {
        let data_store = store.clone();
        let data_ui = branch_ui.clone();
        let data_name = new_branch_name.clone();
        let data_picker = branch_picker.downgrade();
        branch_picker.update(cx, |picker, _cx| {
            picker.set_data_source(Box::new(
                move |filter: &str, window: &Window, cx: &gpui::App| {
                    // the picker hands itself in for the form's repaints;
                    // a dying entity renders an empty list
                    match data_picker.upgrade() {
                        Some(picker) => branch_data(
                            &data_store,
                            &data_ui,
                            &data_name,
                            &picker,
                            filter,
                            window,
                            cx,
                        ),
                        None => PickerData {
                            body: PickerBody::Rows(Vec::new()),
                            selected: Selection::default(),
                            footer: None,
                            fonts: theme::font_tokens(),
                            accent: theme::ACCENT,
                            show_tooltips: false,
                        },
                    }
                },
            ));
        });
    }
    // dismissal (outside click, Escape, a pick) clears the ephemeral state
    branch_picker.update(cx, |picker, cx| {
        picker.set_on_dismiss({
            let ui = branch_ui.clone();
            Box::new(move |_cx| {
                let mut ui = ui.borrow_mut();
                ui.delete_armed = None;
                ui.new_branch_open = false;
            })
        });
        picker.set_on_trailing({
            let ui = branch_ui.clone();
            let store = store.clone();
            Box::new(move |name, cx| {
                let confirmed = {
                    let mut ui = ui.borrow_mut();
                    if ui.delete_armed.as_deref() == Some(name) {
                        ui.delete_armed = None;
                        true
                    } else {
                        ui.delete_armed = Some(name.to_string());
                        false
                    }
                };
                if confirmed {
                    store.update(cx, |s, cx| {
                        s.run_git(GitAction::DeleteBranch(name.to_string()), cx)
                    });
                }
                // true = done: the shell dismisses (no re-entrant update —
                // this closure runs inside the picker's own row listener)
                confirmed
            })
        });
        picker.observe_entity(store, cx);
    });
    branch_picker
}

/// One render's worth of the branch list: locals with their delete trash, the
/// inert "Remote" group label, and — when the form is open — the form as the
/// whole panel body.
fn branch_data(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    filter: &str,
    window: &Window,
    cx: &gpui::App,
) -> PickerData {
    let store_read = store.read(cx);
    let current = store_read
        .selected_entry()
        .and_then(|e| e.git_branch_name.clone())
        .unwrap_or_default();
    let show_tooltips = store_read.settings().general.show_tooltips;
    let armed = branch_ui.borrow().delete_armed.clone();
    let form_open = branch_ui.borrow().new_branch_open;

    if form_open {
        return PickerData {
            body: PickerBody::Replace(new_branch_form(
                store,
                branch_ui,
                new_branch_name,
                picker,
                cx,
            )),
            selected: Selection {
                value: current.clone(),
                label: "—".to_string(),
                badge: None,
            },
            footer: None,
            fonts: theme::font_tokens(),
            accent: theme::ACCENT,
            show_tooltips,
        };
    }

    let filter = filter.to_lowercase();
    let mut rows = Vec::new();
    let mut remotes_labeled = false;
    for branch in &store_read.branches {
        if !filter.is_empty() && !branch.name.to_lowercase().contains(&filter) {
            continue;
        }
        let is_remote = branch.kind == devtools_core::model::GitBranchKind::Remote;
        // locals first, then the inert "Remote" group label (re-emerges
        // before the first matching remote while filtering)
        if is_remote && !remotes_labeled {
            remotes_labeled = true;
            rows.push(PickerRow::Group {
                id: "branch-group-remote".into(),
                label: "Remote".into(),
                selector: Some("branch-group-remote".into()),
            });
        }
        let deletable = !is_remote && branch.name != current;
        rows.push(PickerRow::Item(PickerItem {
            id: format!("branch-row-{}", branch.name),
            selector: format!("branch-row-{}", branch.name),
            value: branch.name.clone(),
            // the full name stays the pick payload; the display label sheds
            // its head only when it would overflow
            label: crate::ui::branch_label(&branch.name, window),
            selected: branch.name == current,
            muted: is_remote,
            trailing_delete: deletable.then(|| TrailingDelete {
                id: format!("branch-delete-{}", branch.name),
                selector: format!("branch-delete-{}", branch.name),
                armed: armed.as_deref() == Some(branch.name.as_str()),
                tooltip: "Delete branch",
            }),
        }));
    }

    PickerData {
        body: PickerBody::Rows(rows),
        selected: Selection {
            label: if current.is_empty() {
                "—".to_string()
            } else {
                crate::ui::branch_label(&current, window)
            },
            badge: None,
            value: current,
        },
        footer: Some(new_branch_footer(
            store,
            branch_ui,
            new_branch_name,
            picker,
            cx,
        )),
        fonts: theme::font_tokens(),
        accent: theme::ACCENT,
        show_tooltips,
    }
}

/// The panel footer: "+ New branch…" opens the form seeded with the repo's
/// branch-name prefix and focused, ready to type.
fn new_branch_footer(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    cx: &gpui::App,
) -> gpui::AnyElement {
    let prefix = store.read(cx).settings().repos.branch_name_prefix.clone();
    let seed = if prefix.is_empty() {
        String::new()
    } else {
        format!("{prefix}/")
    };
    let ui = branch_ui.clone();
    let name = new_branch_name.clone();
    let picker = picker.downgrade();
    div()
        .id("new-branch-row")
        .debug_selector(|| "new-branch-row".into())
        .a11y_button("New branch…")
        .cursor_pointer()
        .rounded_md()
        .px_2()
        .py_1()
        .text_size(px(theme::font_small()))
        .text_color(theme::ACCENT)
        .hover(|s| s.bg(theme::HOVER))
        .on_click(move |_, window, cx| {
            // the form always opens seeded with the repo's prefix and
            // focused, ready to type the branch name
            name.update(cx, |state, cx| {
                state.set_value(seed.as_str(), window, cx);
                state.focus(window, cx);
            });
            ui.borrow_mut().new_branch_open = true;
            let _ = picker.update(cx, |_, cx| cx.notify());
        })
        .child("+ New branch…")
        .into_any_element()
}

/// The open new-branch form: the name input over Cancel/Create, replacing the
/// whole panel body.
fn new_branch_form(
    store: &Entity<Store>,
    branch_ui: &Rc<RefCell<BranchUi>>,
    new_branch_name: &Entity<InputState>,
    picker: &Entity<FilteredPicker>,
    _cx: &gpui::App,
) -> gpui::AnyElement {
    let cancel_ui = branch_ui.clone();
    let create_ui = branch_ui.clone();
    let create_store = store.clone();
    let create_name = new_branch_name.clone();
    let cancel_picker = picker.downgrade();
    let create_picker = picker.downgrade();
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .debug_selector(|| "new-branch-name".into())
                .child(gpui_kit::component::input::Input::new(new_branch_name).small()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    div()
                        .id("new-branch-cancel")
                        .debug_selector(|| "new-branch-cancel".into())
                        .a11y_button("Cancel new branch")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_DIM)
                        .border_1()
                        .border_color(theme::HAIRLINE)
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click(move |_, _, cx| {
                            cancel_ui.borrow_mut().new_branch_open = false;
                            let _ = cancel_picker.update(cx, |_, cx| cx.notify());
                        })
                        .child("Cancel"),
                )
                .child(
                    div()
                        .id("new-branch-create")
                        .debug_selector(|| "new-branch-create".into())
                        .a11y_button("Create branch")
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_size(px(theme::font_small()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(theme::ACCENT)
                        .text_color(theme::ON_ACCENT)
                        .hover(|s| s.opacity(0.9))
                        .on_click(move |_, window, cx| {
                            let name = create_name.read(cx).value().trim().to_string();
                            if name.is_empty() {
                                return;
                            }
                            create_store.update(cx, |s, cx| {
                                s.run_git(
                                    GitAction::CreateBranch {
                                        name,
                                        start: None,
                                        checkout: true,
                                    },
                                    cx,
                                )
                            });
                            // a created branch is a selection: the popup
                            // closes and the form resets
                            create_ui.borrow_mut().new_branch_open = false;
                            create_name.update(cx, |f, cx| f.set_value("", window, cx));
                            let _ = create_picker.update(cx, |p, cx| {
                                p.dismiss(cx);
                            });
                        })
                        .child("Create"),
                ),
        )
        .into_any_element()
}
