//! The Merge section: one stopped operation at a time — its unmerged files on
//! the left, the Incoming and Current panes over the editable Result. The
//! session comes from `warp_station_core::git::merge`; the page seeds the
//! Result from the worktree file, writes it back on Save, and re-parses the
//! buffer after every edit so Save and the staging gate follow the text, not
//! the file it started from.

use crate::store::Store;
use crate::theme;
use crate::ui::{neutral_button, neutral_button_named, primary_button};
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, FocusHandle, FontWeight, MouseButton, Render, SharedString,
    Window, div, px, uniform_list,
};
use gpui_kit::component::input::{Editor, EditorState, RangeDecoration, RangeDecorationCollection};
use std::path::Path;
use std::rc::Rc;
use warp_station_core::git::merge::{
    BufferConflict, LineSpan, MergeFile, MergeOperation, MergeRow, MergeSession, MergeSide,
    WholeFileReason, buffer_conflicts, unresolved_count,
};
use warpui_kit_core::{motion, shell};

/// One source-pane row: the diff's mono line height.
const LINE_H: f32 = 21.0;
/// The line-number gutter's width.
const GUTTER_W: f32 = 46.0;
/// The conflict strip above the Result scrolls past this height.
const STRIP_MAX_H: f32 = 96.0;

/// Which side a block take writes into the buffer.
#[derive(Clone, Copy)]
enum Take {
    Ours,
    Theirs,
    Both,
}

pub struct MergePage {
    store: Entity<Store>,
    /// The session snapshot the page draws; refreshed when the store's
    /// generation moves, so the page does not clone the session per frame.
    session: Option<MergeSession>,
    generation: u64,
    /// The file picked in the conflict list.
    selected: usize,
    /// The selected file's rows and stage lines, cloned once per pick.
    shown: Option<ShownFile>,
    /// The editable Result for the selected file.
    result: Option<ResultPane>,
    /// A file switch was refused because the buffer has unsaved edits.
    switch_blocked: bool,
    /// The conflict strip's keys: Alt+Up/Down walk its blocks.
    strip_focus: FocusHandle,
    /// The block the strip highlights and the keys move.
    active_conflict: usize,
    /// "Take both" writes the current side first while this is true.
    both_current_first: bool,
}

struct ShownFile {
    key: (u64, usize, String),
    rows: Rc<Vec<MergeRow>>,
    ours: Rc<Vec<String>>,
    theirs: Rc<Vec<String>>,
    result: Rc<Vec<String>>,
}

struct ResultPane {
    folder: String,
    path: String,
    editor: Entity<EditorState>,
    /// The buffer as last read from the editor.
    text: String,
    /// The worktree text the buffer was seeded from: dirty is a comparison.
    disk_text: String,
    dirty: bool,
    blocks: Vec<BufferConflict>,
    unresolved: usize,
    error: Option<String>,
    /// The Result editor's conflict tints; the collection tracks edits, but
    /// the page re-sets it after every re-parse.
    decorations: Option<RangeDecorationCollection>,
    /// The worktree file's stamp when the buffer last synced with it.
    stamp: Option<std::time::SystemTime>,
    /// When the page last looked for an outside edit.
    checked_at: std::time::Instant,
    /// The file changed on disk while the buffer had unsaved edits.
    external: bool,
}

impl MergePage {
    pub fn new(store: Entity<Store>, cx: &mut Context<Self>) -> Self {
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            session: None,
            generation: 0,
            selected: 0,
            shown: None,
            result: None,
            switch_blocked: false,
            strip_focus: cx.focus_handle(),
            active_conflict: 0,
            both_current_first: true,
        }
    }

    fn pick_file(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.result.as_ref().is_some_and(|pane| pane.dirty) {
            self.switch_blocked = true;
            cx.notify();
            return;
        }
        self.switch_blocked = false;
        if self.selected == index {
            return;
        }
        self.selected = index;
        self.shown = None;
        self.result = None;
        self.active_conflict = 0;
        cx.notify();
    }

    /// Alt+Up / Alt+Down walk the conflict strip. The strip is the focused
    /// pane when these arrive; the Result editor keeps its own keys.
    fn handle_strip_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) -> bool {
        if !event.keystroke.modifiers.alt || event.keystroke.modifiers.control {
            return false;
        }
        match event.keystroke.key.as_str() {
            "up" => {
                self.step_conflict(-1, cx);
                true
            }
            "down" => {
                self.step_conflict(1, cx);
                true
            }
            _ => false,
        }
    }

    fn step_conflict(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_ref() else {
            return;
        };
        let count = pane.blocks.len();
        if count == 0 {
            return;
        }
        let current = self.active_conflict.min(count - 1) as i32;
        let next = (current + delta).clamp(0, count as i32 - 1) as usize;
        if next != self.active_conflict {
            self.active_conflict = next;
            cx.notify();
        }
    }

    /// Clones the selected file's rows and stage lines once, not per frame.
    fn sync_shown(&mut self) {
        let file = self
            .session
            .as_ref()
            .and_then(|session| session.files.get(self.selected));
        let key = (
            self.generation,
            self.selected,
            file.map(|file| file.path().to_string()).unwrap_or_default(),
        );
        if self.shown.as_ref().is_some_and(|shown| shown.key == key) {
            return;
        }
        self.shown = match file {
            Some(MergeFile::Text(text)) => Some(ShownFile {
                key,
                rows: Rc::new(text.rows.clone()),
                ours: Rc::new(text.ours.clone()),
                theirs: Rc::new(text.theirs.clone()),
                result: Rc::new(text.result.clone()),
            }),
            _ => None,
        };
    }

    /// Seeds the Result editor from the worktree file, once per selected file.
    /// The session's result lines are the fallback when the file cannot be
    /// read (it was deleted under us mid-merge).
    fn ensure_result(
        &mut self,
        folder: &str,
        path: &str,
        fallback: Rc<Vec<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .result
            .as_ref()
            .is_some_and(|pane| pane.folder == folder && pane.path == path)
        {
            return;
        }
        let text = std::fs::read_to_string(Path::new(folder).join(path)).unwrap_or_else(|_| {
            let mut text = fallback.join("\n");
            if !text.is_empty() {
                text.push('\n');
            }
            text
        });
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .line_number(true)
                .folding(false)
                .auto_close(false)
                .smart_indent(false)
        });
        editor.update(cx, |state, cx| state.set_value(&text, window, cx));
        let decorations = editor.update(cx, |state, cx| {
            state.create_range_decorations_collection(result_decorations(&text), cx)
        });
        cx.observe(&editor, move |this, editor, cx| {
            let text = editor.read(cx).value().to_string();
            this.buffer_edited(text, cx);
        })
        .detach();
        self.result = Some(ResultPane {
            folder: folder.to_string(),
            path: path.to_string(),
            editor,
            disk_text: text.clone(),
            text: text.clone(),
            dirty: false,
            blocks: buffer_conflicts(&text),
            unresolved: unresolved_count(&text),
            error: None,
            decorations: Some(decorations),
            stamp: file_stamp(&Path::new(folder).join(path)),
            checked_at: std::time::Instant::now(),
            external: false,
        });
    }

    /// Follows the worktree: a changed file reloads itself while the buffer is
    /// clean, and asks the user to Reload while it has unsaved edits. Checked
    /// at most once a second, since render is the only clock the page has.
    fn check_external(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_mut() else {
            return;
        };
        if pane.checked_at.elapsed() < std::time::Duration::from_secs(1) {
            return;
        }
        pane.checked_at = std::time::Instant::now();
        let path = Path::new(&pane.folder).join(&pane.path);
        let Some(stamp) = file_stamp(&path) else {
            return;
        };
        if Some(stamp) == pane.stamp {
            return;
        }
        if pane.dirty {
            pane.external = true;
            cx.notify();
            return;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let editor = pane.editor.clone();
        editor.update(cx, |state, cx| state.set_value(&text, window, cx));
        pane.text = text.clone();
        pane.disk_text = text.clone();
        pane.dirty = false;
        pane.external = false;
        pane.stamp = Some(stamp);
        pane.blocks = buffer_conflicts(&text);
        pane.unresolved = unresolved_count(&text);
        self.active_conflict = self
            .active_conflict
            .min(pane.blocks.len().saturating_sub(1));
        if let Some(decorations) = pane.decorations.as_ref() {
            decorations.set(result_decorations(&text), cx);
        }
        cx.notify();
    }

    /// Re-parses the buffer after an edit; this is what the count and the
    /// staging gate read.
    fn buffer_edited(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_mut() else {
            return;
        };
        if text == pane.text {
            return;
        }
        pane.dirty = text != pane.disk_text;
        pane.blocks = buffer_conflicts(&text);
        pane.unresolved = unresolved_count(&text);
        pane.text = text;
        self.active_conflict = self
            .active_conflict
            .min(pane.blocks.len().saturating_sub(1));
        if let Some(decorations) = pane.decorations.as_ref() {
            decorations.set(result_decorations(&pane.text), cx);
        }
        cx.notify();
    }

    /// Rewrites one conflict block with the chosen side and re-parses.
    fn take(&mut self, take: Take, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_mut() else {
            return;
        };
        let Some(block) = pane.blocks.get(index).cloned() else {
            return;
        };
        let replacement: Vec<String> = match take {
            Take::Ours => block.ours_text.clone(),
            Take::Theirs => block.theirs_text.clone(),
            Take::Both => {
                let (first, second) = if self.both_current_first {
                    (&block.ours_text, &block.theirs_text)
                } else {
                    (&block.theirs_text, &block.ours_text)
                };
                first.iter().chain(second.iter()).cloned().collect()
            }
        };
        let next = splice_lines(&pane.text, block.result, &replacement);
        let editor = pane.editor.clone();
        editor.update(cx, |state, cx| state.set_value(&next, window, cx));
        pane.text = next.clone();
        pane.dirty = next != pane.disk_text;
        pane.blocks = buffer_conflicts(&next);
        pane.unresolved = unresolved_count(&next);
        self.active_conflict = self
            .active_conflict
            .min(pane.blocks.len().saturating_sub(1));
        if let Some(decorations) = pane.decorations.as_ref() {
            decorations.set(result_decorations(&next), cx);
        }
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.result.as_mut() else {
            return false;
        };
        let path = Path::new(&pane.folder).join(&pane.path);
        match std::fs::write(&path, pane.text.as_bytes()) {
            Ok(()) => {
                pane.disk_text = pane.text.clone();
                pane.dirty = false;
                pane.error = None;
                pane.stamp = file_stamp(&path);
                pane.external = false;
                self.switch_blocked = false;
                cx.notify();
                true
            }
            Err(error) => {
                pane.error = Some(format!("Could not save: {error}"));
                cx.notify();
                false
            }
        }
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_mut() else {
            return;
        };
        let path = Path::new(&pane.folder).join(&pane.path);
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let editor = pane.editor.clone();
                editor.update(cx, |state, cx| state.set_value(&text, window, cx));
                pane.text = text.clone();
                pane.disk_text = text.clone();
                pane.dirty = false;
                pane.error = None;
                pane.stamp = file_stamp(&path);
                pane.external = false;
                pane.blocks = buffer_conflicts(&text);
                pane.unresolved = unresolved_count(&text);
                self.active_conflict = self
                    .active_conflict
                    .min(pane.blocks.len().saturating_sub(1));
                if let Some(decorations) = pane.decorations.as_ref() {
                    decorations.set(result_decorations(&text), cx);
                }
                self.switch_blocked = false;
            }
            Err(error) => pane.error = Some(format!("Could not reload: {error}")),
        }
        cx.notify();
    }

    /// The session's count with the selected file's buffer count swapped in,
    /// so the header and the list follow the edits.
    fn unresolved_total(&self, session: &MergeSession) -> usize {
        let disk = session
            .files
            .get(self.selected)
            .and_then(|file| match file {
                MergeFile::Text(text) => Some(text.unresolved()),
                MergeFile::WholeFile(_) => None,
            });
        match (disk, self.result.as_ref()) {
            (Some(disk), Some(pane)) if self.pane_matches(pane) => {
                session.unresolved() - disk + pane.unresolved
            }
            _ => session.unresolved(),
        }
    }

    fn pane_matches(&self, pane: &ResultPane) -> bool {
        self.session
            .as_ref()
            .and_then(|session| session.files.get(self.selected))
            .is_some_and(|file| file.path() == pane.path)
    }
}

impl Render for MergePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.store
            .update(cx, |store, cx| store.ensure_merge_loaded(cx));
        let (repos, target, generation) = {
            let store = self.store.read(cx);
            (
                store.merge_repos(),
                store.merge_target(),
                store.merge_generation,
            )
        };
        if generation != self.generation {
            self.generation = generation;
            self.session = self.store.read(cx).merge.clone();
            self.selected = 0;
            self.shown = None;
            self.result = None;
            self.switch_blocked = false;
            self.active_conflict = 0;
        }
        let Some(target) = target else {
            return div().size_full().into_any_element();
        };
        if self.session.is_none() {
            let store = self.store.read(cx);
            let still_loading =
                store.merge_loading || store.merge_loaded_for.as_deref() != Some(target.as_str());
            return if still_loading {
                loading(&target)
            } else {
                resolved(&target)
            }
            .into_any_element();
        }
        self.sync_shown();
        let Some(session) = self.session.as_ref() else {
            return loading(&target).into_any_element();
        };

        let unresolved = self.unresolved_total(session);
        let header = self.header(&target, &repos, session, unresolved, cx);
        let list = self.file_list(session, cx);
        let labels = session.operation.side_labels();
        let text_file = match session.files.get(self.selected) {
            Some(MergeFile::Text(text)) => {
                Some((text.path.clone(), text.ours.len(), text.theirs.len()))
            }
            _ => None,
        };
        let whole = match session.files.get(self.selected) {
            Some(MergeFile::WholeFile(whole)) => Some((whole.path.clone(), whole.reason)),
            _ => None,
        };
        let detail = self.detail(&target, labels, text_file, whole, window, cx);
        div()
            .id("merge-page")
            .debug_selector(|| "merge-page".into())
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .child(header)
            .child(shell::panes().child(list).child(detail))
            .into_any_element()
    }
}

impl MergePage {
    fn header(
        &self,
        target: &str,
        repos: &[(String, MergeOperation)],
        session: &MergeSession,
        unresolved: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = div()
            .id("merge-header")
            .debug_selector(|| "merge-header".into())
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_4()
            .h(px(46.))
            .flex_none()
            .border_b_1()
            .border_color(theme::hairline())
            .child(
                div()
                    .text_size(px(theme::font_body()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text())
                    .child("Merge"),
            )
            // the page is already named Merge; the chip earns its place only
            // for the other operations
            .when(session.operation != MergeOperation::Merge, |row| {
                row.child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_dim())
                        .child(session.operation.title()),
                )
            })
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text_muted())
                    .child(repo_name(target)),
            );
        if unresolved > 0 {
            row = row.child(
                div()
                    .px_2()
                    .h(px(20.))
                    .flex()
                    .items_center()
                    .rounded(px(10.))
                    .bg(theme::tint(theme::WARN))
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::text())
                    .child(format!("{unresolved} unresolved")),
            );
        }
        row = row.child(div().flex_1());
        if repos.len() > 1 {
            for (folder, operation) in repos {
                let folder = folder.clone();
                let picked = folder.clone();
                let selected = folder == target;
                row = row.child(
                    div()
                        .id(SharedString::from(format!("merge-repo-{folder}")))
                        .debug_selector(|| "merge-repo-chip".into())
                        .cursor_pointer()
                        .px_2()
                        .h(px(20.))
                        .flex()
                        .items_center()
                        .rounded(px(10.))
                        .text_size(px(theme::font_micro()))
                        .when(selected, |chip| {
                            chip.bg(theme::tint(theme::accent()))
                                .text_color(theme::text())
                        })
                        .when(!selected, |chip| {
                            chip.bg(theme::panel()).text_color(theme::text_dim())
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.result.as_ref().is_some_and(|pane| pane.dirty) {
                                this.switch_blocked = true;
                                cx.notify();
                                return;
                            }
                            this.selected = 0;
                            this.shown = None;
                            this.result = None;
                            this.store.update(cx, |store, cx| {
                                store.select_merge_folder(picked.clone(), cx);
                            });
                        }))
                        .child(format!("{} · {}", repo_name(&folder), operation.title())),
                );
            }
        }
        row.into_any_element()
    }

    fn file_list(&self, session: &MergeSession, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div()
            .id("merge-files")
            .debug_selector(|| "merge-files".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        for (index, file) in session.files.iter().enumerate() {
            rows = rows.child(self.file_row(index, file, cx));
        }
        shell::list_pane(self.store.read(cx).list_width())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .h(px(34.))
                    .flex_none()
                    .border_b_1()
                    .border_color(theme::hairline())
                    .child(
                        div()
                            .text_size(px(theme::font_micro()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::text_muted())
                            .child("UNMERGED FILES"),
                    )
                    .child(
                        div()
                            .text_size(px(theme::font_micro()))
                            .text_color(theme::text_muted())
                            .child(session.files.len().to_string()),
                    ),
            )
            .child(rows)
            .into_any_element()
    }

    fn file_row(&self, index: usize, file: &MergeFile, cx: &mut Context<Self>) -> AnyElement {
        let selected = index == self.selected;
        let (detail, attention) = match file {
            MergeFile::Text(text) => {
                let unresolved = if selected {
                    self.result
                        .as_ref()
                        .map(|pane| pane.unresolved)
                        .unwrap_or_else(|| text.unresolved())
                } else {
                    text.unresolved()
                };
                (format!("{unresolved} unresolved"), unresolved > 0)
            }
            MergeFile::WholeFile(whole) => (whole.reason.label().to_string(), true),
        };
        shell::list_row(SharedString::from(format!("merge-file-{index}")), selected)
            .debug_selector(|| "merge-file-row".into())
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(theme::font_meta()))
                    .text_color(theme::text())
                    .child(file_name(file.path())),
            )
            .child(
                div()
                    .text_size(px(theme::font_micro()))
                    .when(attention, |line| line.text_color(theme::WARN))
                    .when(!attention, |line| line.text_color(theme::text_muted()))
                    .child(detail),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.pick_file(index, cx)))
            .into_any_element()
    }

    fn detail(
        &mut self,
        target: &str,
        labels: (&'static str, &'static str),
        text_file: Option<(String, usize, usize)>,
        whole: Option<(String, WholeFileReason)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some((path, ours_lines, theirs_lines)) = text_file {
            let fallback = self
                .shown
                .as_ref()
                .map(|shown| shown.result.clone())
                .unwrap_or_default();
            self.ensure_result(target, &path, fallback, window, cx);
            self.check_external(window, cx);
            let panes = self.source_panes(labels, &path, ours_lines, theirs_lines, cx);
            let result = self.result_section(labels, cx);
            return div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .min_h_0()
                // the wrapper has to be a flex column or the list inside gets
                // no bounded height and renders no rows at all
                .child(div().flex().flex_col().flex_1().min_h_0().child(panes))
                .child(div().h(px(1.)).flex_none().bg(theme::hairline()))
                .child(div().flex_1().min_h_0().flex().flex_col().child(result))
                .into_any_element();
        }
        if let Some((path, reason)) = whole {
            let entity = cx.entity();
            let ours_entity = entity.clone();
            let theirs_entity = entity.clone();
            let ours_folder = target.to_string();
            let theirs_folder = target.to_string();
            let ours_path = path.clone();
            let theirs_path = path.clone();
            let ours = move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                let folder = ours_folder.clone();
                let path = ours_path.clone();
                ours_entity.update(cx, |this, cx| {
                    this.store.update(cx, |store, cx| {
                        store.take_merge_side(folder, path, MergeSide::Ours, cx)
                    });
                });
            };
            let theirs = move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                let folder = theirs_folder.clone();
                let path = theirs_path.clone();
                theirs_entity.update(cx, |this, cx| {
                    this.store.update(cx, |store, cx| {
                        store.take_merge_side(folder, path, MergeSide::Theirs, cx)
                    });
                });
            };
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child(file_name(&path)),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(reason.label()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(neutral_button(
                            "merge-whole-ours",
                            format!("Take {}", labels.0),
                            true,
                            ours,
                        ))
                        .child(neutral_button(
                            "merge-whole-theirs",
                            format!("Take {}", labels.1),
                            true,
                            theirs,
                        )),
                )
                .into_any_element();
        }
        empty_detail("No file selected")
    }

    fn source_panes(
        &self,
        labels: (&'static str, &'static str),
        path: &str,
        ours_lines: usize,
        theirs_lines: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(shown) = self.shown.as_ref() else {
            return empty_detail("Reading the conflict…");
        };
        let ours = shown.ours.clone();
        let theirs = shown.theirs.clone();
        let rows = shown.rows.clone();
        let id = SharedString::from(format!("merge-panes-{path}-{}", rows.len()));
        let list = uniform_list(
            id,
            rows.len(),
            cx.processor(move |_this, range: std::ops::Range<usize>, _window, _cx| {
                range
                    .map(|index| pane_row(index, &rows, &ours, &theirs))
                    .collect::<Vec<_>>()
            }),
        );
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(pane_head(labels, ours_lines, theirs_lines))
            .child(
                div()
                    .debug_selector(|| "merge-panes".into())
                    .flex_1()
                    .min_h_0()
                    .font_family(theme::code_font())
                    .text_size(px(theme::font_mono_meta()))
                    .child(list.size_full()),
            )
            .into_any_element()
    }

    fn result_section(
        &self,
        labels: (&'static str, &'static str),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(pane) = self.result.as_ref() else {
            return empty_detail("Reading the result…");
        };
        let entity = cx.entity();
        let save_entity = entity.clone();
        let reload_entity = entity.clone();
        let order_entity = entity.clone();
        let dirty = pane.dirty;
        let unresolved = pane.unresolved;

        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(34.))
            .flex_none()
            .border_b_1()
            .border_color(theme::hairline())
            .child(
                div()
                    .text_size(px(theme::font_micro()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::text_muted())
                    .child("RESULT"),
            );
        if unresolved > 0 {
            bar = bar.child(
                div()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::WARN)
                    .child(format!("{unresolved} unresolved")),
            );
        } else {
            bar = bar.child(
                div()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::SUCCESS_BRIGHT)
                    .child("Resolved"),
            );
        }
        if self.switch_blocked {
            bar = bar.child(
                div()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::WARN)
                    .child("Unsaved edits — Save or Reload before switching files"),
            );
        }
        if let Some(error) = pane.error.as_ref() {
            bar = bar.child(
                div()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::DANGER)
                    .child(error.clone()),
            );
        }
        if pane.external {
            bar = bar.child(
                div()
                    .text_size(px(theme::font_micro()))
                    .text_color(theme::WARN)
                    .child("Changed on disk — Reload to take it"),
            );
        }
        bar = bar.child(div().flex_1()).child(neutral_button(
            "merge-order",
            format!("Both: {} first", labels.0),
            true,
            move |_, _, cx| {
                order_entity.update(cx, |this, cx| {
                    this.both_current_first = !this.both_current_first;
                    cx.notify();
                })
            },
        ));
        bar = bar.child(neutral_button(
            "merge-reload",
            "Reload",
            true,
            move |_, window, cx| reload_entity.update(cx, |this, cx| this.reload(window, cx)),
        ));
        bar = bar.child(neutral_button(
            "merge-save",
            "Save",
            dirty,
            move |_, _, cx| {
                save_entity.update(cx, |this, cx| {
                    this.save(cx);
                });
            },
        ));
        let stage_entity = entity.clone();
        let stage_folder = pane.folder.clone();
        let stage_path = pane.path.clone();
        let can_stage = unresolved == 0 && pane.error.is_none();
        bar = bar.child(primary_button(
            "merge-stage",
            "Stage",
            can_stage,
            move |_, _, cx| {
                let folder = stage_folder.clone();
                let path = stage_path.clone();
                stage_entity.update(cx, |this, cx| {
                    if this.save(cx) {
                        this.store
                            .update(cx, |store, cx| store.stage_merge_file(folder, path, cx));
                    }
                });
            },
        ));

        let strip = self.conflict_strip(labels, cx);
        let editor = Editor::new(&pane.editor)
            .bordered(false)
            .appearance(false)
            .h_full()
            .aria_label("Merge result")
            .font_family(SharedString::from(theme::code_font()))
            .text_size(px(theme::font_mono_meta()));
        let mut section = div().flex().flex_col().flex_1().min_h_0().child(bar);
        if !pane.blocks.is_empty() {
            section = section.child(strip);
        }
        section
            .child(div().flex_1().min_h_0().child(editor))
            .into_any_element()
    }

    fn conflict_strip(
        &self,
        labels: (&'static str, &'static str),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(pane) = self.result.as_ref() else {
            return div().into_any_element();
        };
        let mut rows = div()
            .id("merge-conflicts")
            .debug_selector(|| "merge-conflicts".into())
            .flex()
            .flex_col()
            .flex_none()
            .max_h(px(STRIP_MAX_H))
            .overflow_y_scroll()
            .border_b_1()
            .border_color(theme::hairline())
            .track_focus(&self.strip_focus)
            .on_key_down(cx.listener(|this, event, _window, cx| {
                if this.handle_strip_key(event, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.strip_focus.is_focused(window) {
                        window.focus(&this.strip_focus, cx);
                    }
                }),
            );
        for (index, block) in pane.blocks.iter().enumerate() {
            let ours = cx.entity();
            let theirs = cx.entity();
            let both = cx.entity();
            let active = index == self.active_conflict;
            rows = rows.child(
                div()
                    .debug_selector(move || {
                        if active {
                            "merge-conflict-active".to_string()
                        } else {
                            "merge-conflict-row".to_string()
                        }
                    })
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .h(px(30.))
                    .border_b_1()
                    .border_color(theme::hairline())
                    .when(active, |row| row.bg(theme::tint(theme::accent())))
                    .child(
                        div()
                            .text_size(px(theme::font_micro()))
                            .text_color(theme::text_muted())
                            .child(format!(
                                "Conflict {} · line {}",
                                index + 1,
                                block.result.start + 1
                            )),
                    )
                    .child(div().flex_1())
                    .child(neutral_button_named(
                        format!("merge-take-ours-{index}"),
                        labels.0,
                        format!("Take {} for conflict {}", labels.0, index + 1),
                        true,
                        move |_, window, cx| {
                            ours.update(cx, |this, cx| this.take(Take::Ours, index, window, cx))
                        },
                    ))
                    .child(neutral_button_named(
                        format!("merge-take-theirs-{index}"),
                        labels.1,
                        format!("Take {} for conflict {}", labels.1, index + 1),
                        true,
                        move |_, window, cx| {
                            theirs.update(cx, |this, cx| this.take(Take::Theirs, index, window, cx))
                        },
                    ))
                    .child(neutral_button_named(
                        format!("merge-take-both-{index}"),
                        "Both",
                        format!("Take both sides for conflict {}", index + 1),
                        true,
                        move |_, window, cx| {
                            both.update(cx, |this, cx| this.take(Take::Both, index, window, cx))
                        },
                    )),
            );
        }
        rows.into_any_element()
    }
}

fn pane_head(labels: (&'static str, &'static str), ours: usize, theirs: usize) -> AnyElement {
    let half = |label: &'static str, lines: usize| {
        div()
            .flex()
            .items_center()
            .flex_1()
            .min_w_0()
            .px_2()
            .h_full()
            .text_size(px(theme::font_micro()))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme::text_muted())
            .child(format!("{label} · {lines} lines"))
    };
    div()
        .debug_selector(|| "merge-pane-row".into())
        .flex()
        .flex_row()
        .flex_none()
        .h(px(28.))
        .items_center()
        .border_b_1()
        .border_color(theme::hairline())
        .child(half(labels.0, ours))
        .child(div().w(px(1.)).h_full().bg(theme::hairline()))
        .child(half(labels.1, theirs))
        .into_any_element()
}

fn pane_row(index: usize, rows: &[MergeRow], ours: &[String], theirs: &[String]) -> AnyElement {
    let Some(row) = rows.get(index).copied() else {
        return div().into_any_element();
    };
    let ours_text = row
        .ours
        .and_then(|line| ours.get(line as usize))
        .map(String::as_str)
        .unwrap_or("");
    let theirs_text = row
        .theirs
        .and_then(|line| theirs.get(line as usize))
        .map(String::as_str)
        .unwrap_or("");
    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(LINE_H))
        .w_full()
        .when(row.block.is_some(), |line| {
            line.bg(theme::tint(theme::WARN))
        })
        .child(gutter(row.ours))
        .child(code_cell(ours_text))
        .child(div().w(px(1.)).h_full().bg(theme::hairline()))
        .child(gutter(row.theirs))
        .child(code_cell(theirs_text))
        .into_any_element()
}

fn gutter(line: Option<u32>) -> AnyElement {
    div()
        .w(px(GUTTER_W))
        .flex_none()
        .flex()
        .justify_end()
        .pr_2()
        .text_color(theme::text_muted())
        .child(line.map(|line| format!("{}", line + 1)).unwrap_or_default())
        .into_any_element()
}

fn code_cell(text: &str) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .px_2()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_color(theme::text())
        .child(text.to_string())
        .into_any_element()
}

/// Replaces one line span in a text with the given lines, keeping the file's
/// line ending and whether it ended with one.
fn splice_lines(text: &str, span: LineSpan, replacement: &[String]) -> String {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let trailing = text.ends_with('\n');
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let start = (span.start as usize).min(lines.len());
    let end = (span.start as usize + span.len as usize).min(lines.len());
    lines.splice(start..end, replacement.iter().cloned());
    let mut next = lines.join(eol);
    if trailing {
        next.push_str(eol);
    }
    next
}

/// The Result editor's conflict tints, one per complete block still in the
/// buffer.
fn result_decorations(text: &str) -> Vec<RangeDecoration> {
    buffer_conflicts(text)
        .into_iter()
        .filter_map(|block| line_byte_range(text, block.result))
        .map(|range| RangeDecoration::new(range).with_color(theme::tint(theme::WARN)))
        .collect()
}

/// The byte range one line span covers, its newlines included. Ranges are the
/// decoration collection's UTF-8 offsets.
fn line_byte_range(text: &str, span: LineSpan) -> Option<std::ops::Range<usize>> {
    let mut ends = Vec::new();
    let mut at = 0;
    for piece in text.split_inclusive('\n') {
        at += piece.len();
        ends.push(at);
    }
    let start_line = span.start as usize;
    if ends.is_empty() || start_line >= ends.len() {
        return None;
    }
    let start = if start_line == 0 {
        0
    } else {
        ends[start_line - 1]
    };
    let end_line = (start_line + span.len as usize).min(ends.len());
    if end_line == 0 {
        return None;
    }
    let end = ends[end_line - 1];
    (end > start).then_some(start..end)
}

fn loading(target: &str) -> AnyElement {
    div()
        .id("merge-page")
        .debug_selector(|| "merge-page".into())
        .size_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_4()
                .h(px(46.))
                .border_b_1()
                .border_color(theme::hairline())
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Merge"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(repo_name(target)),
                ),
        )
        .child(
            div()
                .debug_selector(|| "merge-loading".into())
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .child(motion::spinner(
                    "merge-loading",
                    "icons/icon-merge.svg",
                    px(16.),
                    theme::text_muted(),
                ))
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child("Reading the stopped operation…"),
                ),
        )
        .into_any_element()
}

/// The stop between the last staged file and the rail item leaving: the index
/// is clean, the probe has not answered yet.
fn resolved(target: &str) -> AnyElement {
    div()
        .id("merge-page")
        .debug_selector(|| "merge-page".into())
        .size_full()
        .flex()
        .flex_col()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_4()
                .h(px(46.))
                .border_b_1()
                .border_color(theme::hairline())
                .child(
                    div()
                        .text_size(px(theme::font_body()))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::text())
                        .child("Merge"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child(repo_name(target)),
                ),
        )
        .child(
            div()
                .debug_selector(|| "merge-resolved".into())
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(theme::font_meta()))
                        .text_color(theme::text_muted())
                        .child("Every conflict is resolved."),
                ),
        )
        .into_any_element()
}

fn empty_detail(text: &str) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(theme::font_meta()))
        .text_color(theme::text_muted())
        .child(text.to_string())
        .into_any_element()
}

fn repo_name(folder: &str) -> String {
    Path::new(folder)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| folder.to_string())
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

/// A file's last write time: the page's signal that another editor touched
/// the worktree under it.
fn file_stamp(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}
