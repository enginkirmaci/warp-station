//! The Notes page: per-repo markdown notes — folder/file tree on the left,
//! editor on the right with Edit/Live/Split/Preview modes, a formatting
//! toolbar over the pure markdown_editing ops, dirty in-memory buffers that
//! survive navigation, and capped search. Opened from the titlebar.

use crate::store::Store;
use crate::theme;
use crate::ui::icon;
use devtools_core::notes as notes_core;
use gpui::prelude::*;
use gpui::{div, px, Context, Entity, FontWeight, SharedString, Window};
use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::component::Sizable as _;
use gpui_kit::base::text::TextView;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq)]
enum NotesMode {
    Edit,
    Split,
    Preview,
}

pub struct NotesPage {
    pub store: Entity<Store>,
    tree: Vec<notes_core::NotesTreeItem>,
    tree_loaded_for: Option<String>,
    expanded: std::collections::HashSet<PathBuf>,
    open_note: Option<PathBuf>,
    /// Unsaved note contents by path — navigation never loses an edit.
    dirty: HashMap<PathBuf, String>,
    mode: NotesMode,
    search: Entity<InputState>,
    search_hits: Vec<notes_core::NotesSearchHit>,
    editor: Entity<TextareaState>,
    /// Delete arming: (path, expire-instant) — a second click within 3s fires.
    armed_delete: Option<(PathBuf, std::time::Instant)>,
}

impl NotesPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes"));
        let editor = cx.new(|cx| TextareaState::new(window, cx).placeholder("# Title"));
        cx.subscribe(&search, |this, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.run_search(field.read(cx).value().to_string(), cx);
            }
        })
        .detach();
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        Self {
            store,
            tree: Vec::new(),
            tree_loaded_for: None,
            expanded: std::collections::HashSet::new(),
            open_note: None,
            dirty: HashMap::new(),
            mode: NotesMode::Split,
            search,
            search_hits: Vec::new(),
            editor,
            armed_delete: None,
        }
    }

    fn notes_root(&self, cx: &Context<Self>) -> Option<PathBuf> {
        let store = self.store.read(cx);
        let settings = store.settings();
        let root = notes_core::resolve_store_root(&settings.general.notes_store_path);
        let repo = store.selected_entry()?;
        Some(notes_core::repo_notes_root(&root, &repo.name))
    }

    fn reload_tree(&mut self, cx: &mut Context<Self>) {
        if let Some(root) = self.notes_root(cx) {
            if self.tree_loaded_for.as_deref() != Some(root.as_os_str().to_str().unwrap_or("")) {
                self.tree = notes_core::load_tree(&root);
                self.tree_loaded_for = root.as_os_str().to_str().map(str::to_string);
            }
        } else {
            self.tree.clear();
            self.tree_loaded_for = None;
        }
    }

    fn open_note(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        // park the current buffer before switching
        if let Some(current) = self.open_note.clone() {
            let text = self.editor.read(cx).value().to_string();
            self.dirty.insert(current, text);
        }
        let content = self
            .dirty
            .get(&path)
            .cloned()
            .or_else(|| notes_core::read_note(&path).ok())
            .unwrap_or_default();
        self.open_note = Some(path);
        self.editor.update(cx, |state, cx| state.set_value(content.as_str(), window, cx));
        cx.notify();
    }

    fn save_current(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.open_note.clone() else { return };
        let Some(root) = self.notes_root(cx) else { return };
        let text = self.editor.read(cx).value().to_string();
        match notes_core::write_note(&path, &text) {
            Ok(()) => {
                self.dirty.remove(&path);
                tracing::info!("saved {}", path.display());
                let _ = root;
            }
            Err(err) => tracing::warn!("note save failed: {err}"),
        }
        cx.notify();
    }

    fn run_search(&mut self, term: String, cx: &mut Context<Self>) {
        if term.trim().is_empty() {
            self.search_hits.clear();
            cx.notify();
            return;
        }
        if let Some(root) = self.notes_root(cx) {
            self.search_hits = notes_core::search(&root, &term);
            cx.notify();
        }
    }

    fn create_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(root) = self.notes_root(cx) else { return };
        let name = format!("note-{}", chrono::Local::now().format("%H%M%S"));
        match notes_core::create_note(&root, &root, &name) {
            Ok(path) => {
                self.tree = notes_core::load_tree(&root);
                self.open_note(path, window, cx);
            }
            Err(err) => tracing::warn!("note create failed: {err}"),
        }
    }

    fn delete_current(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.open_note.clone() else { return };
        let Some(root) = self.notes_root(cx) else { return };
        match &self.armed_delete {
            Some((armed, _)) if *armed == path => {
                match notes_core::delete(&root, &path) {
                    Ok(()) => {
                        self.dirty.remove(&path);
                        self.open_note = None;
                        self.tree = notes_core::load_tree(&root);
                    }
                    Err(err) => tracing::warn!("note delete failed: {err}"),
                }
                self.armed_delete = None;
            }
            _ => {
                self.armed_delete = Some((path, std::time::Instant::now() + std::time::Duration::from_secs(3)));
            }
        }
        cx.notify();
    }

    fn apply_format(&mut self, format: &str, window: &mut Window, cx: &mut Context<Self>) {
        // the kit textarea keeps its own selection; apply over the whole
        // buffer's caret-less form until selection extraction is wired
        let text = self.editor.read(cx).value().to_string();
        let caret = text.chars().count();
        let result = devtools_core::notes::markdown_apply(format, &text, caret, 0);
        self.editor
            .update(cx, |state, cx| state.set_value(result.text.as_str(), window, cx));
        cx.notify();
    }
}

impl Render for NotesPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.reload_tree(cx);
        let preview_text = self.editor.read(cx).value().to_string();
        let has_repo = self.store.read(cx).selected_entry().is_some();
        let dirty = self
            .open_note
            .as_ref()
            .map(|p| self.dirty.contains_key(p))
            .unwrap_or(false);
        let armed = self
            .armed_delete
            .as_ref()
            .map(|(path, deadline)| path == self.open_note.as_ref().unwrap() && *deadline > std::time::Instant::now())
            .unwrap_or(false);
        let note_title = self
            .open_note
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "no note selected".into());

        div()
            .size_full()
            .flex()
            .flex_row()
            .min_h_0()
            .child(self.render_tree(cx))
            .child(
                div()
                    .id("notes-editor-column")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .min_h_0()
                    .child(self.render_toolbar(note_title, dirty, armed, has_repo, window, cx))
                    .child(self.render_editor(preview_text)),
            )
    }
}

impl NotesPage {
    fn render_tree(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let has_repo = self.store.read(cx).selected_entry().is_some();
        if !has_repo {
            return div()
                .id("notes-tree")
                .w(px(240.))
                .h_full()
                .border_r_1()
                .border_color(theme::HAIRLINE)
                .flex()
                .items_center()
                .justify_center()
                .px_2()
                .text_size(px(theme::FONT_SMALL))
                .text_color(theme::TEXT_FAINT)
                .child("Select a repository first")
                .into_any_element();
        }
        let term = self.search.read(cx).value().to_string();
        let mut tree = div()
            .id("notes-tree")
            .w(px(260.))
            .h_full()
            .border_r_1()
            .border_color(theme::HAIRLINE)
            .flex()
            .flex_col()
            .min_h_0()
            .p_2()
            .gap_2()
            .child(Input::new(&self.search).small());

        if !term.trim().is_empty() {
            let mut hits = div().id("notes-hits").flex().flex_col().gap_1().overflow_y_scroll();
            for (index, hit) in self.search_hits.iter().enumerate() {
                hits = hits.child(
                    div()
                        .id(("search-hit", index as u64))
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .hover(|s| s.bg(theme::HOVER))
                        .on_click({
                            let path = hit.path.clone();
                            cx.listener(move |this, _, window, cx| this.open_note(path.clone(), window, cx))
                        })
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .child(div().text_size(px(theme::FONT_SMALL)).text_color(theme::TEXT_DIM).child(hit.title.clone()))
                                .child(
                                    div()
                                        .text_size(px(theme::FONT_SMALL - 2.))
                                        .text_color(theme::TEXT_FAINT)
                                        .child(hit.excerpt.clone()),
                                ),
                        ),
                );
            }
            tree = tree.child(hits);
        } else {
            tree = tree.child(
                div()
                    .id("notes-tree-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.render_tree_items(&self.tree.clone(), 0, cx)),
            );
        }
        tree.into_any_element()
    }

    fn render_tree_items(
        &mut self,
        items: &[notes_core::NotesTreeItem],
        depth: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut list = div().flex().flex_col().gap_0p5();
        for (index, item) in items.iter().enumerate() {
            let expanded = self.expanded.contains(&item.full_path);
            let selected = self.open_note.as_ref() == Some(&item.full_path);
            let id = gpui::ElementId::NamedInteger(
                SharedString::from(format!("note-item-{}-{}", depth, item.name)),
                index as u64,
            );
            let row = div()
                .id(id)
                .cursor_pointer()
                .rounded_md()
                .px_1p5()
                .py_0p5()
                .text_size(px(theme::FONT_SMALL))
                .when(selected, |d| d.bg(theme::tint(theme::ACCENT)).text_color(theme::ACCENT))
                .when(!selected, |d| d.text_color(theme::TEXT_DIM))
                .hover(|s| s.bg(theme::HOVER))
                .on_click({
                    let item = item.clone();
                    cx.listener(move |this, _, window, cx| {
                        if item.is_folder {
                            if this.expanded.contains(&item.full_path) {
                                this.expanded.remove(&item.full_path);
                            } else {
                                this.expanded.insert(item.full_path.clone());
                            }
                        } else {
                            this.open_note(item.full_path.clone(), window, cx);
                        }
                        cx.notify();
                    })
                })
                .child(div()
                    .pl(px(depth as f32 * 10.0))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .when(item.is_folder, |row| {
                        row.child(div().text_size(px(theme::FONT_SMALL - 2.)).text_color(theme::TEXT_FAINT)
                            .child(if expanded { "▾" } else { "▸" }))
                    })
                    .child(if item.is_folder {
                        icon("icons/icon-folder.svg", px(theme::ICON_SMALL - 2.), theme::TEXT_MUTED)
                    } else {
                        icon("icons/icon-code-block.svg", px(theme::ICON_SMALL - 2.), theme::TEXT_MUTED)
                    })
                    .child(item.name.clone()));
            list = list.child(row);
            if item.is_folder && expanded {
                list = list.child(self.render_tree_items(&item.children, depth + 1, cx));
            }
        }
        list.into_any_element()
    }

    fn render_toolbar(
        &mut self,
        note_title: String,
        dirty: bool,
        armed: bool,
        has_repo: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let _ = window;
        let mut modes = div().flex().flex_row().gap_1();
        for (mode, label) in [
            (NotesMode::Edit, "Edit"),
            (NotesMode::Split, "Split"),
            (NotesMode::Preview, "Preview"),
        ] {
            let active = self.mode == mode;
            modes = modes.child(
                div()
                    .id(SharedString::from(format!("notes-mode-{label}")))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_0p5()
                    .text_size(px(theme::FONT_SMALL))
                    .when(active, |d| d.bg(theme::tint(theme::ACCENT)).text_color(theme::ACCENT).font_weight(FontWeight::SEMIBOLD))
                    .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                    .hover(|s| s.opacity(0.85))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mode = mode;
                        cx.notify();
                    }))
                    .child(label),
            );
        }

        let mut formats = div().flex().flex_row().gap_1();
        for (format, glyph) in [
            ("bold", "B"), ("italic", "I"), ("code", "</>"), ("link", "🔗"),
            ("h1", "H1"), ("h2", "H2"), ("bullet", "•"), ("task", "☑"), ("quote", ">"), ("hr", "―"),
        ] {
            formats = formats.child(
                div()
                    .id(SharedString::from(format!("fmt-{format}")))
                    .cursor_pointer()
                    .rounded_md()
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(theme::FONT_SMALL - 1.))
                    .text_color(theme::TEXT_MUTED)
                    .hover(|s| s.bg(theme::HOVER))
                    .on_click({
                        cx.listener(move |this, _, window, cx| this.apply_format(format, window, cx))
                    })
                    .child(glyph),
            );
        }

        div()
            .id("notes-toolbar")
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .py_1p5()
            .border_b_1()
            .border_color(theme::HAIRLINE)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(div().text_size(px(theme::FONT_SMALL)).text_color(theme::TEXT_MUTED).child(note_title.clone()))
                    .when(dirty, |row| row.child(div().text_size(px(theme::FONT_SMALL - 1.)).text_color(theme::AMBER_BRIGHT).child("● unsaved")))
                    .child(div().flex_1())
                    .child(modes)
                    .when(has_repo, |row| {
                        row.child(
                            div()
                                .id("note-new")
                                .cursor_pointer()
                                .rounded_md()
                                .px_2()
                                .py_0p5()
                                .text_size(px(theme::FONT_SMALL))
                                .text_color(theme::ACCENT)
                                .hover(|s| s.bg(theme::tint(theme::ACCENT)))
                                .on_click(cx.listener(|this, _, window, cx| this.create_note(window, cx)))
                                .child("+ note"),
                        )
                        .child(
                            div()
                                .id("note-save")
                                .cursor_pointer()
                                .rounded_md()
                                .px_2()
                                .py_0p5()
                                .text_size(px(theme::FONT_SMALL))
                                .text_color(theme::TEXT_DIM)
                                .border_1()
                                .border_color(theme::HAIRLINE)
                                .hover(|s| s.bg(theme::HOVER))
                                .on_click(cx.listener(|this, _, window, cx| this.save_current(window, cx)))
                                .child("Save"),
                        )
                        .child(
                            div()
                                .id("note-delete")
                                .cursor_pointer()
                                .rounded_md()
                                .px_2()
                                .py_0p5()
                                .text_size(px(theme::FONT_SMALL))
                                .when(armed, |d| d.text_color(theme::DANGER).border_1().border_color(theme::tint(theme::DANGER)))
                                .when(!armed, |d| d.text_color(theme::TEXT_FAINT).border_1().border_color(theme::HAIRLINE))
                                .hover(|s| s.opacity(0.85))
                                .on_click(cx.listener(|this, _, _, cx| this.delete_current(cx)))
                                .child(if armed { "Confirm delete" } else { "Delete" }),
                        )
                    }),
            )
            .when(self.mode != NotesMode::Preview, |toolbar| toolbar.child(formats))
            .into_any_element()
    }

    fn render_editor(&mut self, preview_text: String) -> gpui::AnyElement {
        if self.open_note.is_none() {
            return div()
                .id("notes-empty")
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::FONT_BODY))
                .text_color(theme::TEXT_FAINT)
                .child("Pick a note from the tree, or create one")
                .into_any_element();
        }
        let preview = TextView::markdown(
            gpui::ElementId::Name("notes-preview".into()),
            SharedString::from(preview_text),
        )
        .selectable(true);
        match self.mode {
            NotesMode::Edit => div()
                .id("notes-edit")
                .flex_1()
                .min_h_0()
                .p_2()
                .child(Textarea::new(&self.editor).bordered(false).appearance(false)),
            NotesMode::Preview => div()
                .id("notes-preview-pane")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_3()
                .child(preview),
            NotesMode::Split => div()
                .id("notes-split")
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(
                    div()
                        .id("notes-split-edit")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .p_2()
                        .border_r_1()
                        .border_color(theme::HAIRLINE)
                        .child(Textarea::new(&self.editor).bordered(false).appearance(false)),
                )
                .child(
                    div()
                        .id("notes-split-preview")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .overflow_y_scroll()
                        .p_3()
                        .child(preview),
                ),
        }
        .into_any_element()
    }
}
