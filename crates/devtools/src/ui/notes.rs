//! The Notes page: per-repo markdown notes — folder/file tree on the left,
//! editor on the right with Edit/Live/Split/Preview modes. Live is an
//! Obsidian-style live preview: markdown renders in place while typing and
//! source markers (`**`, backticks, link URLs) fade out wherever the cursor
//! is not on their line. The toolbar drives the pure markdown_editing ops,
//! dirty in-memory buffers survive navigation, and search is capped. Opened
//! from the titlebar.

use crate::store::Store;
use crate::theme;
use crate::ui::icon;
use devtools_core::markdown_live::{self, MarkdownLiveKind};
use devtools_core::notes as notes_core;
use gpui::prelude::*;
use gpui::{
    div, hsla, px, relative, Context, Entity, FontStyle, FontWeight, HighlightStyle,
    MouseButton, SharedString, StrikethroughStyle, UnderlineStyle, Window, WindowControlArea,
};
use gpui_kit::base::text::TextView;
use gpui_kit::component::input::{
    Editor, EditorState, FoldRange, HighlightStyleResolver, Input, InputEdit, InputEvent,
    InputHighlighter, InputHighlighterFactory, InputState, Rope,
};
use gpui_kit::component::Sizable as _;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq)]
enum NotesMode {
    Edit,
    /// Obsidian-style single surface: markdown styled in place while editing;
    /// syntax markers fade unless the cursor is on their line.
    Live,
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
    /// The on-disk contents of the open note; comparing against the editor
    /// makes the unsaved marker follow every keystroke.
    saved_text: String,
    mode: NotesMode,
    search: Entity<InputState>,
    search_hits: Vec<notes_core::NotesSearchHit>,
    /// The shared editor for Edit/Live/Split — the code-editor engine, so
    /// Live can install the markdown highlighter.
    editor: Entity<EditorState>,
    /// Delete arming: (path, expire-instant) — a second click within 3s fires.
    armed_delete: Option<(PathBuf, std::time::Instant)>,
    /// The "+ note" create form (name + optional folder).
    create_open: bool,
    create_name: Entity<InputState>,
    create_folder: Entity<InputState>,
}

impl NotesPage {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search all notes…"));
        let caret = Rc::new(RefCell::new(Some(0..0)));
        let highlighter_caret = caret.clone();
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language(LIVE_LANGUAGE)
                .line_number(false)
                .folding(false)
                .auto_close(false)
                .smart_indent(false)
                .placeholder("# Title");
            // the app's markdown live highlighter owns "markdown"; anything
            // else (the raw Edit/Split panes use LIVE_LANGUAGE's plain
            // sibling) falls back to unstyled text.
            state.set_highlighter_factory(live_highlighter_factory(highlighter_caret), cx);
            state
        });
        let create_name = cx.new(|cx| InputState::new(window, cx).placeholder("New note name"));
        let create_folder = cx.new(|cx| InputState::new(window, cx).placeholder("Folder (optional)"));
        cx.subscribe(&search, |this, field, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.run_search(field.read(cx).value().to_string(), cx);
            }
        })
        .detach();
        // previews and marker-fading follow the editor on every keystroke and
        // every caret move
        let observe_caret = caret.clone();
        cx.observe(&editor, move |_, editor, cx| {
            *observe_caret.borrow_mut() = Some(editor.read(cx).selected_range());
            cx.notify();
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
            saved_text: String::new(),
            mode: NotesMode::Live,
            search,
            search_hits: Vec::new(),
            editor,
            armed_delete: None,
            create_open: false,
            create_name,
            create_folder,
        }
    }

    fn notes_root(&self, cx: &Context<Self>) -> Option<PathBuf> {
        let store = self.store.read(cx);
        let settings = store.settings();
        let root = notes_core::resolve_store_root(&settings.general.notes_store_path);
        // repo-scoped when a repo is selected, whole-store otherwise (the
        // title-bar notes view roots at the store, one folder per repo)
        match store.selected_entry() {
            Some(repo) => Some(notes_core::repo_notes_root(&root, &repo.name)),
            None => Some(root),
        }
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
        let baseline = notes_core::read_note(&path).unwrap_or_default();
        let content = self
            .dirty
            .get(&path)
            .cloned()
            .unwrap_or_else(|| baseline.clone());
        self.saved_text = baseline;
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
                self.saved_text = text;
                tracing::info!("saved {}", path.display());
                let _ = root;
            }
            Err(err) => self.notice(cx, crate::store::NoticeKind::Error, &format!("note save failed: {err}")),
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
        let raw_name = self.create_name.read(cx).value().to_string();
        let raw_folder = self.create_folder.read(cx).value().to_string();
        let Some(name) = notes_core::sanitize_entry_name(raw_name.trim(), true) else {
            self.notice(cx, crate::store::NoticeKind::Info, "Enter a note name");
            return;
        };
        // optional subfolder under the root (sanitized per segment)
        let mut target = root.clone();
        if !raw_folder.trim().is_empty() {
            for segment in raw_folder.trim().split(['/', '\\']) {
                match notes_core::sanitize_entry_name(segment, false) {
                    Some(clean) => target.push(clean),
                    None => {
                        self.notice(cx, crate::store::NoticeKind::Info, "Folder name is not usable");
                        return;
                    }
                }
            }
        }
        match notes_core::create_note(&root, &target, &name) {
            Ok(path) => {
                self.tree = notes_core::load_tree(&root);
                self.create_open = false;
                self.create_name.update(cx, |f, cx| f.set_value(String::new(), window, cx));
                self.create_folder.update(cx, |f, cx| f.set_value(String::new(), window, cx));
                if let Some(parent) = path.parent() {
                    self.expanded.insert(parent.to_path_buf());
                }
                self.open_note(path, window, cx);
            }
            Err(err) => self.notice(cx, crate::store::NoticeKind::Error, &format!("note create failed: {err}")),
        }
    }

    fn notice(&mut self, cx: &mut Context<Self>, kind: crate::store::NoticeKind, text: &str) {
        self.store.update(cx, |s, cx| {
            s.notify(kind, text.to_string());
            cx.notify();
        });
        cx.notify();
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
                        self.saved_text.clear();
                        self.tree = notes_core::load_tree(&root);
                    }
                    Err(err) => self.notice(cx, crate::store::NoticeKind::Error, &format!("note delete failed: {err}")),
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
        // the kit editor owns the selection: read it, apply over the exact
        // range, and restore the op's returned selection
        let text = self.editor.read(cx).value().to_string();
        let range = self.editor.read(cx).selected_range();
        let start_ch = byte_to_char(&text, range.start);
        let end_ch = byte_to_char(&text, range.end);
        let result = devtools_core::notes::markdown_apply(format, &text, start_ch, end_ch - start_ch);
        self.editor.update(cx, |state, cx| {
            // replace_all keeps the undo stack, unlike set_value
            state.replace_all(result.text.as_str(), window, cx);
            let start_b = char_to_byte(&result.text, result.start);
            let end_b = char_to_byte(&result.text, result.start + result.length);
            state.set_selected_range(start_b..end_b, cx);
        });
        cx.notify();
    }

    /// Live styles the markdown in place; the raw panes show plain text.
    /// Switching resets the editor's syntax engine on the next refresh.
    fn apply_editor_language(&mut self, cx: &mut Context<Self>) {
        let language = if self.mode == NotesMode::Live { LIVE_LANGUAGE } else { RAW_LANGUAGE };
        self.editor.update(cx, |state, cx| state.set_highlighter(language, cx));
    }

    /// The editor's current buffer — for headless probes.
    #[doc(hidden)]
    pub fn editor_value(&self, cx: &gpui::App) -> String {
        self.editor.read(cx).value().to_string()
    }
}

fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].chars().count()
}

fn char_to_byte(text: &str, char_ix: usize) -> usize {
    text.char_indices().nth(char_ix).map(|(i, _)| i).unwrap_or(text.len())
}

impl Render for NotesPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.reload_tree(cx);
        // a global-search click parked a note to open
        if let Some(path) = self.store.update(cx, |store, _| store.pending_note_open.take()) {
            let note_path = path.clone();
            self.open_note(note_path, window, cx);
            let root = self.notes_root(cx);
            while let Some(parent) = path.parent() {
                if let Some(root) = &root
                    && parent.starts_with(root) {
                        self.expanded.insert(parent.to_path_buf());
                    }
                if parent.parent().is_none() {
                    break;
                }
            }
        }
        let preview_text = self.editor.read(cx).value().to_string();
        let dirty = self.open_note.is_some() && preview_text != self.saved_text;
        let armed = self
            .armed_delete
            .as_ref()
            .map(|(path, deadline)| {
                Some(path) == self.open_note.as_ref() && *deadline > std::time::Instant::now()
            })
            .unwrap_or(false);
        let note_title = self
            .open_note
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "No note selected".into());

        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(theme::BACKGROUND)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let control = event.keystroke.modifiers.control;
                if control && event.keystroke.key == "s" {
                    this.save_current(window, cx);
                    cx.stop_propagation();
                } else if control && event.keystroke.key == "n" {
                    this.create_open = true;
                    this.create_name.update(cx, |field, cx| field.focus(window, cx));
                    cx.notify();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape" && this.create_open {
                    this.create_open = false;
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .child(self.render_titlebar(window, cx))
            .child(
                div()
                    .id("notes-body")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .px_2()
                    .pb_2()
                    .child(self.render_tree(cx))
                    .child(
                        div()
                            .id("notes-editor-column")
                            .debug_selector(|| "notes-editor-column".into())
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .flex_col()
                            .min_h_0()
                            .rounded_lg()
                            .border_1()
                            .border_color(theme::HAIRLINE)
                            .bg(theme::CARD)
                            .child(self.render_toolbar(note_title, dirty, armed, window, cx))
                            .child(self.render_editor(preview_text, cx))
                            .when(self.open_note.is_some(), |column| {
                                column.child(self.render_status_bar(cx))
                            }),
                    ),
            )
    }
}

impl NotesPage {
    /// The floating window paints its own title bar: the window is created
    /// with appears_transparent, so without this strip there is no title,
    /// drag region, or window controls.
    fn render_titlebar(&mut self, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let show_tooltips = self.store.read(cx).settings().general.show_tooltips;
        div()
            .h(px(38.))
            .flex()
            .items_center()
            .px_2()
            .gap_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                gpui_kit::base::GlobalState::suppress_text_selection(cx);
            })
            .child(icon(
                "icons/icon-note.svg",
                px(theme::icon_medium()),
                theme::TEXT_MUTED,
            ))
            .child(
                div()
                    .text_size(px(theme::font_title()))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Notes"),
            )
            .when_some(self.store.read(cx).selected_entry().map(|e| e.name.clone()), |row, repo| {
                row.child(
                    div()
                        .rounded_md()
                        .bg(theme::CHIP)
                        .px_1p5()
                        .py_0p5()
                        .text_size(px(theme::font_small() - 1.))
                        .text_color(theme::TEXT_MUTED)
                        .child(repo),
                )
            })
            .child(
                div()
                    .flex_1()
                    .h_full()
                    // Windows drags via the hit-test path (this hitbox answers
                    // HTCAPTION); start_window_move is a no-op there. Linux
                    // keeps the explicit move call.
                    .window_control_area(WindowControlArea::Drag)
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.start_window_move();
                    }),
            )
            .child(crate::ui::window::window_button(
                "notes-minimize",
                "icons/icon-window-minimize.svg",
                theme::HOVER,
                "Minimize",
                show_tooltips,
                |_, window, _| window.minimize_window(),
            ))
            .child(crate::ui::window::window_button(
                "notes-zoom",
                if window.is_maximized() {
                    "icons/icon-window-restore.svg"
                } else {
                    "icons/icon-window-maximize.svg"
                },
                theme::HOVER,
                if window.is_maximized() { "Restore" } else { "Maximize" },
                show_tooltips,
                cx.listener(|_, _, window, cx| {
                    window.zoom_window();
                    // the compositor toggles asynchronously — nudge a
                    // re-render so the maximize/restore glyph keeps up
                    cx.notify();
                }),
            ))
            .child(crate::ui::window::window_button(
                "notes-close",
                "icons/icon-window-close.svg",
                theme::tint(theme::DANGER),
                "Close",
                show_tooltips,
                |_, window, _| window.remove_window(),
            ))
            .into_any_element()
    }

    /// The status strip: "N words · N chars · N lines" and "Ln x, Col y".
    fn render_status_bar(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let text = if self.open_note.is_some() { self.editor.read(cx).value().to_string() } else { String::new() };
        let words = text.split_whitespace().count();
        let chars = text.chars().count();
        let lines = text.lines().count();
        let dirty = self.open_note.is_some() && text != self.saved_text;
        let (line, col) = if self.open_note.is_some() {
            let byte = self.editor.read(cx).selected_range().start;
            let prefix = &text[..byte.min(text.len())];
            let line = prefix.matches('\n').count() + 1;
            let col = prefix.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
            (line, col)
        } else {
            (1, 1)
        };
        div()
            .id("notes-status")
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_2()
            .py_1()
            .border_t_1()
            .border_color(theme::HAIRLINE)
            .text_size(px(theme::font_small() - 1.))
            .text_color(theme::TEXT_FAINT)
            .child(format!("{words} words · {chars} chars · {lines} lines"))
            .child(div().flex_1())
            .when(dirty, |row| {
                row.child(
                    div()
                        .text_color(theme::AMBER_BRIGHT)
                        .child("Unsaved · Ctrl+S to save"),
                )
            })
            .child(format!("Ln {line}, Col {col}"))
            .into_any_element()
    }

    fn render_tree(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let term = self.search.read(cx).value().to_string();
        let can_create = self.store.read(cx).selected_entry().is_some() || !self.tree.is_empty();
        let mut tree = div()
            .id("notes-sidebar")
            .debug_selector(|| "notes-sidebar".into())
            .w(px(300.))
            .h_full()
            .flex()
            .flex_col()
            .min_h_0()
            .gap_2()
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::CARD)
            // search row: field, "+" new note, collapse-all
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                                if event.keystroke.key == "escape" {
                                    this.search.update(cx, |field, cx| field.set_value(String::new(), window, cx));
                                    this.search_hits.clear();
                                    cx.notify();
                                }
                            }))
                            .child(Input::new(&self.search).small().cleanable(true).prefix(icon(
                                "icons/icon-search.svg",
                                px(theme::icon_small() - 1.),
                                theme::TEXT_FAINT,
                            ))),
                    )
                    .when(can_create, |row| {
                        row.child(
                            div()
                                .id("note-new")
                                .cursor_pointer()
                                .rounded_md()
                                .size(px(22.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .hover(|s| s.bg(theme::HOVER))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.create_open = !this.create_open;
                                    cx.notify();
                                }))
                                .child(icon("icons/icon-plus.svg", px(theme::icon_small() - 1.), theme::TEXT_MUTED)),
                        )
                    })
                    .child(
                        div()
                            .id("notes-collapse-all")
                            .cursor_pointer()
                            .rounded_md()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|s| s.bg(theme::HOVER))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.expanded.clear();
                                cx.notify();
                            }))
                            .child(icon("icons/icon-list-bullet.svg", px(theme::icon_small() - 1.), theme::TEXT_MUTED)),
                    ),
            )
            // inline "new note" form, stacked for the narrow column
            .when(self.create_open, |card| {
                card.child(
                    div()
                        .id("note-create-form")
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(Input::new(&self.create_name).small())
                        .child(Input::new(&self.create_folder).small())
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .child(
                                    div()
                                        .id("note-create-confirm")
                                        .cursor_pointer()
                                        .rounded_md()
                                        .px_2()
                                        .py_0p5()
                                        .text_size(px(theme::font_small()))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .bg(theme::ACCENT)
                                        .text_color(theme::ON_ACCENT)
                                        .hover(|s| s.opacity(0.9))
                                        .on_click(cx.listener(|this, _, window, cx| this.create_note(window, cx)))
                                        .child("Create"),
                                )
                                .child(
                                    div()
                                        .id("note-create-cancel")
                                        .cursor_pointer()
                                        .rounded_md()
                                        .px_2()
                                        .py_0p5()
                                        .text_size(px(theme::font_small()))
                                        .text_color(theme::TEXT_FAINT)
                                        .hover(|s| s.bg(theme::HOVER))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.create_open = false;
                                            cx.notify();
                                        }))
                                        .child("×"),
                                ),
                        ),
                )
            });

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
                                .child(div().text_size(px(theme::font_small())).text_color(theme::TEXT_DIM).child(hit.title.clone()))
                                .child(
                                    div()
                                        .text_size(px(theme::font_small() - 2.))
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
            let selector_name = item.name.clone();
            let row = div()
                .id(id)
                .debug_selector(move || format!("note-item-{}", selector_name))
                .cursor_pointer()
                .rounded_md()
                .px_1p5()
                .py_0p5()
                .text_size(px(theme::font_small()))
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
                        row.child(icon(
                            if expanded {
                                "icons/icon-chevron-down.svg"
                            } else {
                                "icons/icon-chevron-right.svg"
                            },
                            px(theme::icon_small() - 4.),
                            theme::TEXT_FAINT,
                        ))
                    })
                    .child(if item.is_folder {
                        icon("icons/icon-folder.svg", px(theme::icon_small() - 2.), theme::TEXT_MUTED)
                    } else {
                        icon("icons/icon-note.svg", px(theme::icon_small() - 2.), theme::TEXT_MUTED)
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let _ = window;
        // segmented mode control: Edit / Live / Split / Preview
        let mut modes = div()
            .flex()
            .flex_row()
            .gap_0p5()
            .p_0p5()
            .rounded_md()
            .border_1()
            .border_color(theme::HAIRLINE)
            .bg(theme::BACKGROUND);
        for (mode, label) in [
            (NotesMode::Edit, "Edit"),
            (NotesMode::Live, "Live"),
            (NotesMode::Split, "Split"),
            (NotesMode::Preview, "Preview"),
        ] {
            let active = self.mode == mode;
            modes = modes.child(
                div()
                    .id(SharedString::from(format!("notes-mode-{label}")))
                    .debug_selector(move || format!("notes-mode-{label}"))
                    .cursor_pointer()
                    .rounded_md()
                    .px_2()
                    .py_0p5()
                    .text_size(px(theme::font_small()))
                    .when(active, |d| d.bg(theme::CHIP).text_color(theme::TEXT).font_weight(FontWeight::SEMIBOLD))
                    .when(!active, |d| d.text_color(theme::TEXT_MUTED))
                    .hover(|s| s.opacity(0.85))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.mode = mode;
                        this.apply_editor_language(cx);
                        cx.notify();
                    }))
                    .child(label),
            );
        }

        // formatting row: text styles, headings, lists and inserts, separated
        // into groups; icon buttons where the asset set has a glyph and short
        // text labels for the rest (B/I/S/H1–H3)
        let mut formats = div().flex().flex_row().items_center().gap_0p5();
        let spec: [(&str, &str, Option<&str>, bool); 14] = [
            ("bold", "B", None, false),
            ("italic", "I", None, false),
            ("strike", "S", None, false),
            ("code", "", Some("icons/icon-code.svg"), false),
            ("link", "", Some("icons/icon-link.svg"), false),
            ("h1", "H1", None, true),
            ("h2", "H2", None, false),
            ("h3", "H3", None, false),
            ("bullet", "", Some("icons/icon-list-bullet.svg"), true),
            ("numbered", "", Some("icons/icon-list-ordered.svg"), false),
            ("task", "", Some("icons/icon-task.svg"), false),
            ("quote", "", Some("icons/icon-quote.svg"), false),
            ("codeblock", "", Some("icons/icon-code-block.svg"), true),
            ("hr", "—", None, false),
        ];
        for (format, label, icon_path, separator) in spec {
            if separator {
                formats = formats.child(
                    div()
                        .mx_1()
                        .h(px(14.))
                        .w(px(1.))
                        .bg(theme::HAIRLINE),
                );
            }
            let mut button = div()
                .id(SharedString::from(format!("fmt-{format}")))
                .cursor_pointer()
                .rounded_md()
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(theme::font_small() - 1.))
                .text_color(theme::TEXT_MUTED)
                .hover(|s| s.bg(theme::HOVER).text_color(theme::TEXT))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.apply_format(format, window, cx)
                }));
            if let Some(path) = icon_path {
                button = button.child(icon(path, px(theme::icon_small() - 1.), theme::TEXT_MUTED));
            } else {
                button = button.child(label);
            }
            formats = formats.child(button);
        }

        div()
            .id("notes-toolbar")
            .debug_selector(|| "notes-toolbar".into())
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
                    // filename header (mono, like a document title strip)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(SharedString::from(theme::code_font()))
                            .text_size(px(theme::font_small()))
                            .text_color(theme::TEXT_MUTED)
                            .child(note_title.clone()),
                    )
                    .when(dirty, |row| row.child(div().text_size(px(theme::font_small() - 1.)).text_color(theme::AMBER_BRIGHT).child("● unsaved")))
                    .child(div().flex_1())
                    .child(modes)
                    .when(self.open_note.is_some(), |row| {
                        row.child(
                            div()
                                .id("note-save")
                                .cursor_pointer()
                                .rounded_md()
                                .px_2()
                                .py_0p5()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .text_size(px(theme::font_small()))
                                .text_color(theme::TEXT_DIM)
                                .border_1()
                                .border_color(theme::HAIRLINE)
                                .hover(|s| s.bg(theme::HOVER))
                                .on_click(cx.listener(|this, _, window, cx| this.save_current(window, cx)))
                                .child(icon(
                                    "icons/icon-save.svg",
                                    px(theme::icon_small() - 1.),
                                    theme::TEXT_MUTED,
                                ))
                                .child("Save"),
                        )
                        .child(
                            div()
                                .id("note-delete")
                                .cursor_pointer()
                                .size(px(24.))
                                .rounded_md()
                                .flex()
                                .items_center()
                                .justify_center()
                                .when(armed, |d| d.border_1().border_color(theme::tint(theme::DANGER)))
                                .when(!armed, |d| d.hover(|s| s.bg(theme::HOVER)))
                                .on_click(cx.listener(|this, _, _, cx| this.delete_current(cx)))
                                .child(icon(
                                    "icons/icon-trash.svg",
                                    px(theme::icon_small() - 1.),
                                    if armed { theme::DANGER } else { theme::TEXT_FAINT },
                                )),
                        )
                    }),
            )
            .when(
                self.open_note.is_some() && self.mode != NotesMode::Preview,
                |toolbar| toolbar.child(formats),
            )
            .into_any_element()
    }

    /// The one editor element shared by the editing modes, in the UI font —
    /// the kit Editor defaults to mono, refined away here.
    fn editor_element(&self) -> Editor {
        Editor::new(&self.editor)
            .bordered(false)
            .appearance(false)
            .h_full()
            .font_family(SharedString::from(theme::ui_font()))
            .text_size(px(theme::font_body() + 1.))
            .line_height(relative(1.7))
    }

    fn render_editor(&mut self, preview_text: String, cx: &mut Context<Self>) -> gpui::AnyElement {
        if self.open_note.is_none() {
            let can_create =
                self.store.read(cx).selected_entry().is_some() || !self.tree.is_empty();
            return div()
                .id("notes-empty")
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .child(icon(
                    "icons/icon-note.svg",
                    px(theme::icon_large() + 8.),
                    theme::TEXT_FAINT,
                ))
                .child(
                    div()
                        .text_size(px(theme::font_body() + 1.))
                        .text_color(theme::TEXT_MUTED)
                        .child("No note open"),
                )
                .child(
                    div()
                        .text_size(px(theme::font_small()))
                        .text_color(theme::TEXT_FAINT)
                        .child("Pick one from the tree, or start a new note."),
                )
                .when(can_create, |column| {
                    column.child(
                        div()
                            .id("notes-empty-new")
                            .cursor_pointer()
                            .rounded_md()
                            .border_1()
                            .border_color(theme::ACCENT)
                            .px_3()
                            .py_1p5()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .text_size(px(theme::font_small()))
                            .text_color(theme::ACCENT)
                            .hover(|s| s.bg(theme::tint(theme::ACCENT)))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.create_open = true;
                                this.create_name
                                    .update(cx, |field, cx| field.focus(window, cx));
                                cx.notify();
                            }))
                            .child(icon(
                                "icons/icon-plus.svg",
                                px(theme::icon_small()),
                                theme::ACCENT,
                            ))
                            .child("New note"),
                    )
                })
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
                .debug_selector(|| "notes-edit".into())
                .flex_1()
                .min_h_0()
                .p_2()
                .child(self.editor_element()),
            NotesMode::Preview => div()
                .id("notes-preview-pane")
                .debug_selector(|| "notes-preview-pane".into())
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .p_3()
                .child(preview),
            NotesMode::Live => div()
                .id("notes-live")
                .debug_selector(|| "notes-live".into())
                .flex_1()
                .min_h_0()
                .p_2()
                .child(self.editor_element()),
            NotesMode::Split => div()
                .id("notes-split")
                .debug_selector(|| "notes-split".into())
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(
                    div()
                        .id("notes-split-edit")
                        .debug_selector(|| "notes-split-edit".into())
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .p_2()
                        .border_r_1()
                        .border_color(theme::HAIRLINE)
                        .child(self.editor_element()),
                )
                .child(
                    div()
                        .id("notes-split-preview")
                        .debug_selector(|| "notes-split-preview".into())
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

// ---- the markdown live highlighter -------------------------------------------

/// Language name that triggers the app markdown live styling.
const LIVE_LANGUAGE: &str = "markdown";
/// Anything the factory does not own falls back to plain text.
const RAW_LANGUAGE: &str = "plaintext";

/// How far source markers fade when the cursor is on another line.
const MARKER_FADE: f32 = 0.8;
/// Slightly deeper fade for whole fence lines.
const FENCE_FADE: f32 = 0.85;

/// Builds the editor's highlighter factory: `markdown` gets the live engine,
/// every other language stays unstyled. A free function so the closure
/// coerces to the higher-ranked `dyn Fn(&str)` without inference trouble.
fn live_highlighter_factory(caret: Rc<RefCell<Option<Range<usize>>>>) -> InputHighlighterFactory {
    Rc::new(move |language: &str| {
        (language == LIVE_LANGUAGE).then(|| {
            Box::new(MarkdownLiveHighlighter::new(caret.clone())) as Box<dyn InputHighlighter>
        })
    })
}

/// The Notes live editor's syntax engine: maps the core parser's spans onto
/// editor text styles, fading source markers on lines the caret is not on.
struct MarkdownLiveHighlighter {
    spans: Vec<markdown_live::MarkdownLiveSpan>,
    line_starts: Vec<usize>,
    len: usize,
    /// The editor caret; the page keeps it current on every keystroke and
    /// caret move, so each frame's layout fades against the live position.
    caret: Rc<RefCell<Option<Range<usize>>>>,
}

impl MarkdownLiveHighlighter {
    fn new(caret: Rc<RefCell<Option<Range<usize>>>>) -> Self {
        Self { spans: Vec::new(), line_starts: Vec::new(), len: 0, caret }
    }

    fn style_for(kind: MarkdownLiveKind) -> HighlightStyle {
        let mut style = HighlightStyle::default();
        match kind {
            MarkdownLiveKind::HeadingMarker => {
                style.color = Some(theme::ACCENT.into());
                style.font_weight = Some(FontWeight::SEMIBOLD);
            }
            MarkdownLiveKind::HeadingText => {
                style.font_weight = Some(FontWeight::BOLD);
            }
            MarkdownLiveKind::Emphasis { bold, italic, strike } => {
                if bold {
                    style.font_weight = Some(FontWeight::BOLD);
                }
                if italic {
                    style.font_style = Some(FontStyle::Italic);
                }
                if strike {
                    style.strikethrough = Some(StrikethroughStyle {
                        thickness: px(1.),
                        color: Some(theme::TEXT_MUTED.into()),
                    });
                    style.color = Some(theme::TEXT_MUTED.into());
                }
            }
            MarkdownLiveKind::InlineCode => {
                style.background_color = Some(theme::CHIP.into());
                style.color = Some(theme::GOLD_BRIGHT.into());
            }
            MarkdownLiveKind::CodeFence => {
                style.background_color = Some(hsla(0., 0., 0., 0.16));
                style.color = Some(theme::TEXT_FAINT.into());
            }
            MarkdownLiveKind::CodeBlock => {
                style.background_color = Some(hsla(0., 0., 0., 0.16));
            }
            MarkdownLiveKind::LinkText => {
                style.color = Some(theme::ACCENT.into());
                style.underline = Some(UnderlineStyle {
                    thickness: px(1.),
                    color: Some(theme::ACCENT.into()),
                    wavy: false,
                });
            }
            MarkdownLiveKind::QuoteText => {
                style.font_style = Some(FontStyle::Italic);
                style.color = Some(theme::TEXT_DIM.into());
            }
            MarkdownLiveKind::TaskDone => {
                style.color = Some(theme::SUCCESS_BRIGHT.into());
            }
            // everything quiet shares the faint ink
            MarkdownLiveKind::Marker
            | MarkdownLiveKind::LinkUrl
            | MarkdownLiveKind::QuoteMarker
            | MarkdownLiveKind::ListMarker
            | MarkdownLiveKind::TaskOpen
            | MarkdownLiveKind::HorizontalRule => {
                style.color = Some(theme::TEXT_FAINT.into());
            }
            MarkdownLiveKind::Plain => {}
        }
        style
    }

    /// Which kinds are source syntax that disappears when not being edited.
    fn fades(kind: MarkdownLiveKind) -> Option<f32> {
        match kind {
            MarkdownLiveKind::CodeFence => Some(FENCE_FADE),
            MarkdownLiveKind::Marker | MarkdownLiveKind::LinkUrl => Some(MARKER_FADE),
            _ => None,
        }
    }

    /// The line the caret is on, as a byte range.
    fn caret_line(&self) -> Option<Range<usize>> {
        let caret = self.caret.borrow().as_ref()?.start.min(self.len);
        let idx = self.line_starts.partition_point(|&start| start <= caret).checked_sub(1)?;
        let start = self.line_starts[idx];
        let end = self.line_starts.get(idx + 1).copied().unwrap_or(self.len);
        Some(start..end.max(start))
    }
}

impl InputHighlighter for MarkdownLiveHighlighter {
    fn language(&self) -> SharedString {
        LIVE_LANGUAGE.into()
    }

    fn update(
        &mut self,
        _edit: Option<InputEdit>,
        text: &Rope,
        _folding: bool,
        _window: &mut Window,
        _cx: &mut Context<EditorState>,
    ) {
        // notes are small; a full reparse per edit keeps offsets trivially
        // correct (and the engine re-drives this on every text change)
        let text = text.to_string();
        self.line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        self.len = text.len();
        self.spans = markdown_live::parse(&text);
    }

    fn styles(
        &self,
        range: &Range<usize>,
        _resolver: &dyn HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let caret_line = self.caret_line();
        let mut out: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
        let push_run = |out: &mut Vec<(Range<usize>, HighlightStyle)>,
                            start: usize,
                            end: usize,
                            kind: MarkdownLiveKind| {
            let mut style = Self::style_for(kind);
            if let Some(fade) = Self::fades(kind)
                && caret_line.as_ref().is_some_and(|line| !(start < line.end && end > line.start))
            {
                style.fade_out = Some(fade);
            }
            // adjacent runs with one style shape as one
            if let Some((last_range, last_style)) = out.last_mut()
                && *last_style == style
                && last_range.end == start
            {
                last_range.end = end;
            } else {
                out.push((start..end, style));
            }
        };
        let mut pos = range.start;
        for span in &self.spans {
            if span.range.end <= range.start {
                continue;
            }
            if span.range.start >= range.end {
                break;
            }
            let start = span.range.start.max(range.start);
            let end = span.range.end.min(range.end);
            if start > pos {
                push_run(&mut out, pos, start, MarkdownLiveKind::Plain);
            }
            push_run(&mut out, start, end, span.kind);
            pos = end;
        }
        if pos < range.end {
            push_run(&mut out, pos, range.end, MarkdownLiveKind::Plain);
        }
        out
    }

    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}
