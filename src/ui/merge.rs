//! The merge editor inside the repository detail: one stopped operation at a
//! time — its unmerged files on the left, the Incoming and Current panes over
//! the Result. Each conflict line carries a pick box; the Result is composed
//! from the picks over the file's original marker text, so a pick never
//! depends on what the buffer already says. Save writes the composed text, and
//! the staging gate counts the conflicts nothing was picked for.

use crate::store::Store;
use crate::theme;
use crate::ui::{icon, neutral_button, primary_button};
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, FocusHandle, FontWeight, MouseButton, Render, ScrollHandle,
    SharedString, Window, div, px,
};
use gpui_kit::component::input::{Editor, EditorState, RangeDecoration, RangeDecorationCollection};
use std::path::Path;
use std::rc::Rc;
use warp_station_core::git::merge::{
    BufferConflict, ConflictBlock, LineSpan, MergeFile, MergeOperation, MergeRow, MergeSession,
    MergeSide, WholeFileReason, align_side_lines, buffer_conflicts,
};
use warpui_kit_core::a11y::A11y;
use warpui_kit_core::controls::icon_button;
use warpui_kit_core::{motion, shell};
use warpui_kit_diff::{AlignedOptions, AlignedRow, AlignedSide, render_aligned, row_height};

/// The pick box's cell, left of each gutter in both panes; it matches the
/// kit's aligned view leading cell.
const PICK_W: f32 = 22.0;

/// Which pane a pick box belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Ours,
    Theirs,
}

/// One conflict's picks: a flag per line of each side. The Result is built
/// from them row by row, so both sides can contribute to one block.
#[derive(Clone)]
struct BlockPick {
    ours: Vec<bool>,
    theirs: Vec<bool>,
}

impl BlockPick {
    /// A fresh pick keeps the Current side: the branch's own lines are the
    /// starting point, and Incoming lines are what the user reaches for.
    fn fresh(block: &BufferConflict) -> Self {
        Self {
            ours: vec![true; block.ours_text.len()],
            theirs: vec![false; block.theirs_text.len()],
        }
    }

    fn flip(&mut self, side: Side, index: usize) {
        let list = match side {
            Side::Ours => &mut self.ours,
            Side::Theirs => &mut self.theirs,
        };
        if let Some(flag) = list.get_mut(index) {
            *flag = !*flag;
        }
    }

    fn picked(&self, side: Side, index: usize) -> bool {
        match side {
            Side::Ours => self.ours.get(index).copied().unwrap_or(false),
            Side::Theirs => self.theirs.get(index).copied().unwrap_or(false),
        }
    }

    fn count(&self) -> usize {
        self.ours.iter().filter(|on| **on).count() + self.theirs.iter().filter(|on| **on).count()
    }

    /// The sides of the picked lines, in the same order [`Self::chosen`]
    /// returns them: the Result's decoration bands follow this.
    fn chosen_sides(&self, block: &BufferConflict) -> Vec<Side> {
        let mut sides = Vec::new();
        for (ours, theirs) in align_side_lines(&block.ours_text, &block.theirs_text) {
            if ours.is_some_and(|index| self.picked(Side::Ours, index)) {
                sides.push(Side::Ours);
            }
            if theirs.is_some_and(|index| self.picked(Side::Theirs, index)) {
                sides.push(Side::Theirs);
            }
        }
        sides
    }

    /// The block's lines in visual row order: each row's picked Current line
    /// first, then its picked Incoming line.
    fn chosen(&self, block: &BufferConflict) -> Vec<String> {
        let mut lines = Vec::new();
        for (ours, theirs) in align_side_lines(&block.ours_text, &block.theirs_text) {
            if let Some(index) = ours.filter(|index| self.picked(Side::Ours, *index))
                && let Some(line) = block.ours_text.get(index)
            {
                lines.push(line.clone());
            }
            if let Some(index) = theirs.filter(|index| self.picked(Side::Theirs, *index))
                && let Some(line) = block.theirs_text.get(index)
            {
                lines.push(line.clone());
            }
        }
        lines
    }
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
    /// The Result for the selected file.
    result: Option<ResultPane>,
    /// A file switch was refused because the buffer has unsaved edits.
    switch_blocked: bool,
    /// The conflict strip's keys: Alt+Up/Down walk its blocks.
    panes_focus: FocusHandle,
    /// The block the strip highlights and the keys move.
    active_conflict: usize,
    /// The panes' vertical viewport, so conflict navigation can scroll it.
    panes_scroll: ScrollHandle,
    /// The panes' last offset, so the Result can follow a wheel on the panes
    /// without the mirror re-triggering itself.
    last_panes_y: f32,
}

struct ShownFile {
    key: (u64, usize, String),
    rows: Rc<Vec<MergeRow>>,
    ours: Rc<Vec<String>>,
    theirs: Rc<Vec<String>>,
    result: Rc<Vec<String>>,
    blocks: Rc<Vec<ConflictBlock>>,
}

struct ResultPane {
    folder: String,
    path: String,
    editor: Entity<EditorState>,
    /// The worktree text the picks are applied to: the file with its markers.
    base: String,
    /// The conflicts `base` holds, in file order; the picks follow them.
    origin: Vec<BufferConflict>,
    picks: Vec<BlockPick>,
    /// The composed buffer as last read from the editor.
    text: String,
    /// The last composed text that was saved; dirty is a comparison.
    saved: String,
    /// One band per picked line in the Result, colored by the side it came
    /// from; re-set after every compose.
    decorations: Option<RangeDecorationCollection>,
    dirty: bool,
    unresolved: usize,
    error: Option<String>,
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
            panes_focus: cx.focus_handle(),
            active_conflict: 0,
            panes_scroll: ScrollHandle::new(),
            last_panes_y: 0.,
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
        // the new file starts at its own top, not at the old one's offset
        self.panes_scroll.set_offset(gpui::point(px(0.), px(0.)));
        self.last_panes_y = 0.;
        cx.notify();
    }

    /// Alt+Up / Alt+Down walk the conflicts. The panes are the focused pane
    /// when these arrive; the Result editor keeps its own keys.
    fn handle_panes_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !event.keystroke.modifiers.alt || event.keystroke.modifiers.control {
            return false;
        }
        match event.keystroke.key.as_str() {
            "up" => {
                self.step_conflict(-1, window, cx);
                true
            }
            "down" => {
                self.step_conflict(1, window, cx);
                true
            }
            _ => false,
        }
    }

    fn step_conflict(&mut self, delta: i32, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_ref() else {
            return;
        };
        let count = pane.origin.len();
        if count == 0 {
            return;
        }
        let current = self.active_conflict.min(count - 1) as i32;
        let next = (current + delta).clamp(0, count as i32 - 1) as usize;
        if next != self.active_conflict {
            self.active_conflict = next;
        }
        self.scroll_to_conflict(window, cx);
        cx.notify();
    }

    /// Brings the active conflict into view on both scroll surfaces: the
    /// aligned panes to the block's first row, the Result to the block's line
    /// in the composed text. The three surfaces share one position because
    /// this is the only place that moves them.
    fn scroll_to_conflict(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_ref() else {
            return;
        };
        let Some(shown) = self.shown.as_ref() else {
            return;
        };
        let index = self.active_conflict;
        if pane.origin.get(index).is_none() {
            return;
        }
        let row_h = row_height(theme::font_mono_meta());
        if let Some(row_index) = shown
            .rows
            .iter()
            .position(|row| row.block == Some(index as u32))
        {
            let x = self.panes_scroll.offset().x;
            let target = row_h * row_index as f32;
            self.panes_scroll.set_offset(gpui::point(x, target));
            self.last_panes_y = f32::from(target);
        }
        let line = composed_line(pane, index);
        let editor = pane.editor.clone();
        let line_h = editor.read(cx).line_height().unwrap_or(row_h);
        editor.update(cx, |state, cx| {
            state.set_scroll_offset(gpui::point(px(0.), -(line_h * line as f32)), cx);
        });
    }

    /// The Result follows the panes: a wheel over the panes moves the editor
    /// to the same line. The direction is deliberately one-way — the editor
    /// recentres itself whenever its text is re-composed, and mirroring that
    /// back would yank the panes to the top on every pick.
    fn sync_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_ref() else {
            return;
        };
        let row_h = f32::from(row_height(theme::font_mono_meta()));
        let line_h = f32::from(
            pane.editor
                .read(cx)
                .line_height()
                .unwrap_or_else(|| px(theme::font_mono_meta())),
        );
        if row_h <= 0. || line_h <= 0. {
            return;
        }
        let pane_y = (-f32::from(self.panes_scroll.offset().y)).max(0.);
        if (pane_y - self.last_panes_y).abs() <= 0.5 {
            return;
        }
        let target = (pane_y / row_h) * line_h;
        let editor = pane.editor.clone();
        editor.update(cx, |state, cx| {
            state.set_scroll_offset(gpui::point(px(0.), px(-target)), cx);
        });
        self.last_panes_y = pane_y;
    }

    /// Puts the Result back on the line the panes show after a compose reset
    /// the editor's own scroll: a pick must not move the reader's place.
    fn restore_result_scroll(&mut self, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_ref() else {
            return;
        };
        let row_h = f32::from(row_height(theme::font_mono_meta()));
        let line_h = f32::from(
            pane.editor
                .read(cx)
                .line_height()
                .unwrap_or_else(|| px(theme::font_mono_meta())),
        );
        if row_h <= 0. || line_h <= 0. {
            return;
        }
        let pane_y = (-f32::from(self.panes_scroll.offset().y)).max(0.);
        let target = (pane_y / row_h) * line_h;
        let editor = pane.editor.clone();
        editor.update(cx, |state, cx| {
            state.set_scroll_offset(gpui::point(px(0.), px(-target)), cx);
        });
        self.last_panes_y = pane_y;
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
                blocks: Rc::new(text.blocks.clone()),
            }),
            _ => None,
        };
    }

    /// Seeds the Result from the worktree file, once per selected file. The
    /// picks start on the Current side and the composed text follows them.
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
        let base = std::fs::read_to_string(Path::new(folder).join(path)).unwrap_or_else(|_| {
            let mut text = fallback.join("\n");
            if !text.is_empty() {
                text.push('\n');
            }
            text
        });
        let origin = buffer_conflicts(&base);
        let picks: Vec<BlockPick> = origin.iter().map(BlockPick::fresh).collect();
        let text = compose(&base, &origin, &picks);
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .line_number(true)
                .folding(false)
                .auto_close(false)
                .smart_indent(false)
        });
        editor.update(cx, |state, cx| state.set_value(&text, window, cx));
        let decorations = editor.update(cx, |state, cx| {
            state
                .create_range_decorations_collection(result_decorations(&text, &origin, &picks), cx)
        });
        cx.observe(&editor, move |this, editor, cx| {
            let value = editor.read(cx).value().to_string();
            this.buffer_edited(value, cx);
        })
        .detach();
        let unresolved = count_unresolved(&origin, &picks);
        self.result = Some(ResultPane {
            folder: folder.to_string(),
            path: path.to_string(),
            editor,
            base,
            origin,
            picks,
            text: text.clone(),
            saved: text,
            dirty: false,
            unresolved,
            decorations: Some(decorations),
            error: None,
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
        let Ok(base) = std::fs::read_to_string(&path) else {
            return;
        };
        seed_pane(pane, base, window, cx);
        self.active_conflict = self
            .active_conflict
            .min(pane.origin.len().saturating_sub(1));
        cx.notify();
    }

    /// Follows the editor: typing makes the buffer dirty; the picks stay as
    /// the strip shows them until the user turns one.
    fn buffer_edited(&mut self, text: String, cx: &mut Context<Self>) {
        let Some(pane) = self.result.as_mut() else {
            return;
        };
        if text == pane.text {
            return;
        }
        pane.dirty = text != pane.saved;
        pane.text = text;
        if let Some(decorations) = pane.decorations.as_ref() {
            decorations.set(Vec::new(), cx);
        }
        cx.notify();
    }

    /// Turns one line's pick and recomposes the Result from the picks. This is
    /// the only way a conflict is resolved: no side is written wholesale.
    fn toggle_line(
        &mut self,
        block: usize,
        side: Side,
        local: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let count = {
            let Some(pane) = self.result.as_mut() else {
                return;
            };
            let Some(pick) = pane.picks.get_mut(block) else {
                return;
            };
            pick.flip(side, local);
            let text = compose(&pane.base, &pane.origin, &pane.picks);
            let saved = pane.saved.clone();
            let editor = pane.editor.clone();
            let unresolved = count_unresolved(&pane.origin, &pane.picks);
            editor.update(cx, |state, cx| state.set_value(&text, window, cx));
            if let Some(decorations) = pane.decorations.as_ref() {
                decorations.set(result_decorations(&text, &pane.origin, &pane.picks), cx);
            }
            pane.text = text.clone();
            pane.dirty = text != saved;
            pane.unresolved = unresolved;
            pane.origin.len()
        };
        self.active_conflict = self.active_conflict.min(count.saturating_sub(1));
        self.restore_result_scroll(cx);
        cx.notify();
    }

    fn save(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.result.as_mut() else {
            return false;
        };
        let path = Path::new(&pane.folder).join(&pane.path);
        match std::fs::write(&path, pane.text.as_bytes()) {
            Ok(()) => {
                pane.saved = pane.text.clone();
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
            Ok(base) => {
                seed_pane(pane, base, window, cx);
                self.active_conflict = self
                    .active_conflict
                    .min(pane.origin.len().saturating_sub(1));
                self.switch_blocked = false;
            }
            Err(error) => pane.error = Some(format!("Could not reload: {error}")),
        }
        cx.notify();
    }

    /// The session's count with the selected file's picks swapped in, so the
    /// header and the list follow the choices.
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
        // after the detail seeded the pane: its picks decide the counts the
        // header and the file list show on this same frame
        let Some(session) = self.session.as_ref() else {
            return empty_detail("Reading the conflict…");
        };
        let unresolved = self.unresolved_total(session);
        let header = self.header(&target, &repos, session, unresolved, cx);
        let list = self.file_list(session, cx);
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
        let close_entity = cx.entity();
        row = row.child(neutral_button(
            "merge-close",
            "Close",
            true,
            move |_, _, cx| {
                close_entity.update(cx, |this, cx| {
                    this.store
                        .update(cx, |store, cx| store.close_merge_editor(cx));
                });
            },
        ));
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
        let aria = format!("{}, {detail}", file_name(file.path()));
        shell::list_row(SharedString::from(format!("merge-file-{index}")), selected)
            .debug_selector(|| "merge-file-row".into())
            .a11y_button(aria)
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
            self.sync_scroll(cx);
            let panes = self.source_panes(labels, &path, ours_lines, theirs_lines, cx);
            let result = self.result_section(cx);
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
        &mut self,
        labels: (&'static str, &'static str),
        _path: &str,
        ours_lines: usize,
        theirs_lines: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(shown) = self.shown.as_ref() else {
            return empty_detail("Reading the conflict…");
        };
        let rows = shown.rows.clone();
        let ours = shown.ours.clone();
        let theirs = shown.theirs.clone();
        let blocks = shown.blocks.clone();
        let picks = self
            .result
            .as_ref()
            .map(|pane| pane.picks.clone())
            .unwrap_or_default();
        // the kit draws the aligned columns; the pick boxes are its opt-in
        // leading cells, so nothing else in the kit grows a control column
        // the lead cells are built by the kit after this call returns nothing,
        // so the closure owns what it needs and reaches the page through its
        // entity rather than borrowing the render context
        let entity = cx.entity();
        let mut lead = {
            let blocks = blocks.clone();
            let picks = picks.clone();
            move |side: AlignedSide, line: Option<u32>| -> Option<AnyElement> {
                let (block, pick_side, local) = map_pick_line(&blocks, side, line?)?;
                let checked = picks
                    .get(block)
                    .is_some_and(|pick| pick.picked(pick_side, local));
                let side_name = match pick_side {
                    Side::Ours => "ours",
                    Side::Theirs => "theirs",
                };
                let selector = format!("merge-pick-{block}-{side_name}-{local}");
                let pick_entity = entity.clone();
                Some(
                    div()
                        .id(SharedString::from(selector.clone()))
                        .debug_selector(move || selector.clone())
                        .w(px(PICK_W))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .on_click(
                            move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut gpui::App| {
                                pick_entity.update(cx, |this, cx| {
                                    this.toggle_line(block, pick_side, local, window, cx)
                                });
                            },
                        )
                        .child(pick_box(checked))
                        .into_any_element(),
                )
            }
        };
        // only the rows the panes' viewport can show: a 10k-line file draws
        // a few dozen divs, and the padding keeps the scrollbar honest
        // gpui's offset grows negative as the body scrolls down; the window
        // wants the scrolled distance
        let scrolled = (-f32::from(self.panes_scroll.offset().y)).max(0.);
        let (start, end) = visible_window(
            rows.len(),
            scrolled,
            f32::from(self.panes_scroll.bounds().size.height),
            f32::from(row_height(theme::font_mono_meta())),
        );
        let aligned: Vec<AlignedRow> = rows[start..end]
            .iter()
            .map(|row| AlignedRow {
                left: row.ours.and_then(|index| {
                    ours.get(index as usize)
                        .map(|text| (index + 1, text.as_str()))
                }),
                right: row.theirs.and_then(|index| {
                    theirs
                        .get(index as usize)
                        .map(|text| (index + 1, text.as_str()))
                }),
                changed: row.block.is_some(),
            })
            .collect();
        let lead_ref: &mut dyn FnMut(AlignedSide, Option<u32>) -> Option<AnyElement> = &mut lead;
        let body = render_aligned(
            &aligned,
            AlignedOptions {
                lead: Some(lead_ref),
                font_detail: theme::font_mono_meta(),
                padding: (start, rows.len() - end),
            },
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
                    .id("merge-panes-scroll")
                    .debug_selector(|| "merge-panes".into())
                    .role(gpui::Role::Group)
                    .aria_label("Merge source panes, Current and Incoming")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.panes_scroll)
                    .track_focus(&self.panes_focus)
                    .on_key_down(cx.listener(|this, event, window, cx| {
                        if this.handle_panes_key(event, window, cx) {
                            cx.stop_propagation();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            if !this.panes_focus.is_focused(window) {
                                window.focus(&this.panes_focus, cx);
                            }
                        }),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    fn result_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(pane) = self.result.as_ref() else {
            return empty_detail("Reading the result…");
        };
        let entity = cx.entity();
        let save_entity = entity.clone();
        let reload_entity = entity.clone();
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
        let prev_entity = entity.clone();
        let next_entity = entity.clone();
        bar = bar
            .child(div().flex_1())
            .child(
                icon_button("merge-prev", "Previous conflict")
                    .debug_selector(|| "merge-prev".into())
                    .on_click(move |_, window, cx| {
                        prev_entity.update(cx, |this, cx| this.step_conflict(-1, window, cx));
                    })
                    .child(icon(
                        "icons/icon-arrow-up.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            )
            .child(
                icon_button("merge-next", "Next conflict")
                    .debug_selector(|| "merge-next".into())
                    .on_click(move |_, window, cx| {
                        next_entity.update(cx, |this, cx| this.step_conflict(1, window, cx));
                    })
                    .child(icon(
                        "icons/icon-arrow-down.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            )
            .child(
                icon_button("merge-reload", "Reload the worktree file")
                    .debug_selector(|| "merge-reload".into())
                    .on_click(move |_, window, cx| {
                        reload_entity.update(cx, |this, cx| this.reload(window, cx))
                    })
                    .child(icon(
                        "icons/icon-refresh.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            )
            .child(
                icon_button("merge-save", "Save the result")
                    .debug_selector(|| "merge-save".into())
                    .on_click(move |_, _, cx| {
                        save_entity.update(cx, |this, cx| {
                            this.save(cx);
                        });
                    })
                    .child(icon(
                        "icons/icon-save.svg",
                        px(theme::icon_small()),
                        theme::text_dim(),
                    )),
            );
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

        let editor = Editor::new(&pane.editor)
            .bordered(false)
            .appearance(false)
            .h_full()
            .aria_label("Merge result")
            .font_family(SharedString::from(theme::code_font()))
            .text_size(px(theme::font_mono_meta()));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(bar)
            .child(div().flex_1().min_h_0().child(editor))
            .into_any_element()
    }
}

/// The pick a line number maps to: the aligned row carries a file line, the
/// picks are keyed by block and the block's own line index.
/// How many rows the panes' viewport can show, with a margin either side so a
/// wheel step never exposes a blank band. The offset and viewport come from
/// the panes' own scroll handle; a not-yet-laid-out viewport draws a window
/// of [`VIRTUAL_WINDOW_FALLBACK`] rows.
fn visible_window(total: usize, offset: f32, viewport: f32, row_h: f32) -> (usize, usize) {
    if total == 0 {
        return (0, 0);
    }
    let row_h = row_h.max(1.);
    let offset = offset.max(0.);
    let first = (offset / row_h).floor() as usize;
    let visible = if viewport > 0. {
        (viewport / row_h).ceil() as usize + 1
    } else {
        VIRTUAL_WINDOW_FALLBACK
    };
    let start = first.saturating_sub(VIRTUAL_OVERSCAN);
    let end = first
        .saturating_add(visible)
        .saturating_add(VIRTUAL_OVERSCAN)
        .min(total);
    (start, end)
}

/// Rows drawn above and below the viewport's own range.
const VIRTUAL_OVERSCAN: usize = 8;
/// The window a first frame draws before the viewport has a height.
const VIRTUAL_WINDOW_FALLBACK: usize = 200;

/// The line one conflict starts at in the composed text: the block's own
/// start, shifted by the lines the earlier blocks gave back or took.
fn composed_line(pane: &ResultPane, index: usize) -> usize {
    let mut cursor = 0usize;
    let mut line = 0usize;
    for (block_index, block) in pane.origin.iter().enumerate() {
        let start = block.result.start as usize;
        line += start.saturating_sub(cursor);
        cursor = start + block.result.len as usize;
        if block_index == index {
            return line;
        }
        line += pane
            .picks
            .get(block_index)
            .map(BlockPick::count)
            .unwrap_or(0);
    }
    line
}

/// One band per picked line in the composed text, in the side's color: the
/// Result shows where every line it holds came from, like the panes.
fn result_decorations(
    text: &str,
    origin: &[BufferConflict],
    picks: &[BlockPick],
) -> Vec<RangeDecoration> {
    let mut bands = Vec::new();
    let mut cursor = 0usize;
    let mut line = 0usize;
    for (block, pick) in origin.iter().zip(picks) {
        let start = block.result.start as usize;
        line += start.saturating_sub(cursor);
        cursor = start + block.result.len as usize;
        for side in pick.chosen_sides(block) {
            if let Some(range) = line_byte_range(
                text,
                LineSpan {
                    start: line as u32,
                    len: 1,
                },
            ) {
                let color = match side {
                    Side::Ours => theme::DIFF_DEL_BG,
                    Side::Theirs => theme::DIFF_ADD_BG,
                };
                bands.push(RangeDecoration::new(range).with_color(color));
            }
            line += 1;
        }
    }
    bands
}

/// The byte range one line span covers, its newlines included.
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

fn map_pick_line(
    blocks: &[ConflictBlock],
    side: AlignedSide,
    line: u32,
) -> Option<(usize, Side, usize)> {
    let zero = line.saturating_sub(1) as usize;
    for (index, block) in blocks.iter().enumerate() {
        let span = match side {
            AlignedSide::Left => block.ours,
            AlignedSide::Right => block.theirs,
        };
        let Some(span) = span else {
            continue;
        };
        let start = span.start as usize;
        if zero >= start && zero < start + span.len as usize {
            let pick_side = match side {
                AlignedSide::Left => Side::Ours,
                AlignedSide::Right => Side::Theirs,
            };
            return Some((index, pick_side, zero - start));
        }
    }
    None
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
        .child(div().w(px(PICK_W)).flex_none())
        .child(half(labels.0, ours))
        .child(div().w(px(1.)).h_full().bg(theme::hairline()))
        .child(div().w(px(PICK_W)).flex_none())
        .child(half(labels.1, theirs))
        .into_any_element()
}

/// The 14-square pick box; the app's check glyph when the line is taken.
fn pick_box(checked: bool) -> AnyElement {
    let frame = div()
        .w(px(14.))
        .h(px(14.))
        .rounded(px(3.))
        .border_1()
        .border_color(if checked {
            theme::accent()
        } else {
            theme::text_muted()
        })
        .when(checked, |frame| frame.bg(theme::accent()))
        .flex()
        .items_center()
        .justify_center();
    if checked {
        frame
            .child(crate::ui::icon(
                "icons/icon-check.svg",
                px(10.),
                theme::on_accent(),
            ))
            .into_any_element()
    } else {
        frame.into_any_element()
    }
}

/// Rebuilds a pane's picks from a new marker text: the picks, the composed
/// buffer and the editor all follow it.
fn seed_pane(pane: &mut ResultPane, base: String, window: &mut Window, cx: &mut gpui::App) {
    let origin = buffer_conflicts(&base);
    let picks: Vec<BlockPick> = origin.iter().map(BlockPick::fresh).collect();
    let text = compose(&base, &origin, &picks);
    let editor = pane.editor.clone();
    editor.update(cx, |state, cx| state.set_value(&text, window, cx));
    if let Some(decorations) = pane.decorations.as_ref() {
        decorations.set(result_decorations(&text, &origin, &picks), cx);
    }
    pane.base = base;
    pane.origin = origin;
    pane.picks = picks;
    pane.text = text.clone();
    pane.saved = text;
    pane.dirty = false;
    pane.unresolved = count_unresolved(&pane.origin, &pane.picks);
    pane.error = None;
    pane.stamp = file_stamp(&Path::new(&pane.folder).join(&pane.path));
    pane.checked_at = std::time::Instant::now();
    pane.external = false;
}

/// The composed text: each conflict block replaced by its picked lines, from
/// the end so the earlier spans stay valid.
fn compose(base: &str, origin: &[BufferConflict], picks: &[BlockPick]) -> String {
    let mut out = base.to_string();
    for (block, pick) in origin.iter().zip(picks.iter()).rev() {
        out = splice_lines(&out, block.result, &pick.chosen(block));
    }
    out
}

/// How many conflicts have no picked line at all: the staging gate.
fn count_unresolved(origin: &[BufferConflict], picks: &[BlockPick]) -> usize {
    origin
        .iter()
        .zip(picks)
        .filter(|(_, pick)| pick.count() == 0)
        .count()
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

/// The stop between the last staged file and the editor leaving: the index is
/// clean, the probe has not answered yet.
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

#[cfg(test)]
mod window_tests {
    use super::visible_window;

    #[test]
    fn the_window_follows_the_scroll_offset() {
        assert_eq!(visible_window(0, 0., 210., 21.), (0, 0));
        assert_eq!(visible_window(1000, 0., 210., 21.), (0, 19));
        assert_eq!(visible_window(1000, 420., 210., 21.), (12, 39));
        // near the end the window stays inside the row count
        assert_eq!(visible_window(1000, 20_000., 210., 21.), (944, 971));
        assert_eq!(visible_window(1000, 20_000., 210., 21.).1, 971);
    }

    #[test]
    fn a_first_frame_before_layout_draws_the_fallback() {
        assert_eq!(visible_window(1000, 0., 0., 21.), (0, 208));
        assert_eq!(visible_window(50, 0., 0., 21.), (0, 50));
    }
}
