//! Station's document markdown rendering through the suite's own live
//! markdown engine (warpui-kit-markdown) in read-only mode — the same
//! rendered surface chat answers (warp-agent) and notes use: no caret, no
//! text input, but text selection and copy, clickable links and
//! tree-sitter-highlighted code blocks stay. The PR comment list stays on
//! gpui-kit's TextView deliberately: it is a volatile list of many small
//! comments, not a document, and one editor entity per comment buys nothing.
//!
//! A TextView that renders a platform-supplied body goes through
//! [`text_view_markdown`]: `TextView::markdown` parses during layout with no
//! catch, so one text that trips the `markdown` crate would abort the frame.

use gpui::{
    AnyElement, Entity, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window,
    div, px, relative,
};
use warpui_kit_markdown::{LiveEditor, LiveEditorState};

use crate::theme;

/// Creates the read-only editor hosting `text` and wires height chasing: the
/// render helper reads the editor's height through `&App`, which registers no
/// dependency, so the owning view is notified on editor moves explicitly.
/// One per document surface (README, summary); feed later content with
/// [`feed`].
pub fn attach<C: 'static>(
    text: SharedString,
    window: &mut Window,
    cx: &mut gpui::Context<C>,
) -> Entity<LiveEditorState> {
    warpui_kit_markdown::read_only_document(text, window, cx)
}

/// Swaps the document the editor renders (a regenerated summary, a saved
/// README): `set_value` forgets undo history and resets scroll, which is the
/// wanted behavior for a whole-document replace.
pub fn feed<C: gpui::AppContext>(
    editor: &Entity<LiveEditorState>,
    text: SharedString,
    window: &mut Window,
    cx: &mut C,
) {
    editor.update(cx, |state, cx| state.set_value(text, window, cx));
}

/// The laid-out editor: sized to its content (the surrounding pane scrolls;
/// the editor never becomes an inner scroll area).
pub fn render_read_only(editor: &Entity<LiveEditorState>, cx: &gpui::App) -> AnyElement {
    render(editor, cx, false)
}

/// A remote body or comment as a markdown `TextView`, under the kit's parser
/// guard. A pull request description or comment is platform text nobody has
/// vetted for the parser, so a text that would panic during layout renders as
/// plain prose instead of taking the window down.
pub fn text_view_markdown(id: gpui::ElementId, text: impl Into<SharedString>) -> AnyElement {
    let text = text.into();
    if warpui_kit_markdown::renders_as_markdown(&text) {
        gpui_kit::base::text::TextView::markdown(id, text)
            .selectable(true)
            .into_any_element()
    } else {
        div().child(text).into_any_element()
    }
}

/// Like [`render_read_only`], but for a document embedded in a card: the
/// card already bounds the width, so the engine's readable measure and top
/// margin only add blank space around the content.
pub fn render_read_only_card(editor: &Entity<LiveEditorState>, cx: &gpui::App) -> AnyElement {
    render(editor, cx, true)
}

fn render(editor: &Entity<LiveEditorState>, cx: &gpui::App, in_card: bool) -> AnyElement {
    let height = editor.read(cx).content_height().max(1.);
    let mut element = LiveEditor::new(editor)
        .read_only(true)
        .fit_content(true)
        .text_size(px(theme::font_name()))
        // the document takes no focus, so the window's Ctrl+C is the only
        // keyboard copy; the menu's Copy row prints the key it answers to
        .menu_keys(warpui_kit_markdown::MenuKeys {
            copy: Some("Ctrl C".into()),
            select_all: None,
        });
    if in_card {
        element = element.unbounded_column().top_margin_em(0.5);
    }
    // the design's running text is 15 / 25; the viewer takes a text step but
    // no line-height hook, so the container that owns the document states
    // the ratio
    div()
        .w_full()
        .h(px(height))
        .text_size(px(theme::font_name()))
        .line_height(relative(25. / 15.))
        .child(element)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::text_view_markdown;

    /// The `markdown` 1.0.0 compiler bug the guard exists for: a diff of
    /// deleted Markdown spells this out exactly — its empty lines read `-`,
    /// its rules `----`. gpui parses during layout, where nothing catches it.
    const PANICS: &str = "-a\n-\n----\n@@ -1 +0,0 @@\n----";

    #[test]
    fn a_body_that_panics_the_parser_takes_the_prose_branch() {
        assert!(!warpui_kit_markdown::renders_as_markdown(PANICS));
        // the helper builds the fallback for that text without asking the
        // parser again (the guard memoizes the verdict)
        let _fallback = text_view_markdown(gpui::ElementId::Name("probe-body".into()), PANICS);
    }

    #[test]
    fn ordinary_markdown_still_renders_through_the_text_view() {
        assert!(warpui_kit_markdown::renders_as_markdown(
            "# Title\n\n- one\n- two\n\n`code`\n"
        ));
    }
}
