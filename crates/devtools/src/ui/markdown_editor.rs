//! The shared markdown live-highlighter for code-editor states: the Notes
//! page's live editor and the Settings prompt-template editor install the
//! same factory so both render markdown in place, fading source markers on
//! lines the caret is not on.

use crate::theme;
use devtools_core::markdown_live::{self, MarkdownLiveKind};
use gpui::{hsla, px, Context, FontStyle, FontWeight, HighlightStyle, SharedString, StrikethroughStyle, UnderlineStyle, Window};
use gpui_kit::component::input::{
    EditorState, FoldRange, HighlightStyleResolver, InputEdit, InputHighlighter,
    InputHighlighterFactory, Rope,
};
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Language name that triggers the app markdown live styling.
pub(crate) const LIVE_LANGUAGE: &str = "markdown";

/// How far source markers fade when the cursor is on another line.
const MARKER_FADE: f32 = 0.85;

/// Builds the editor's highlighter factory: `markdown` gets the live engine,
/// every other language stays unstyled. A free function so the closure
/// coerces to the higher-ranked `dyn Fn(&str)` without inference trouble.
pub(crate) fn live_highlighter_factory(caret: Rc<RefCell<Option<Range<usize>>>>) -> InputHighlighterFactory {
    Rc::new(move |language: &str| {
        (language == LIVE_LANGUAGE).then(|| {
            Box::new(MarkdownLiveHighlighter::new(caret.clone())) as Box<dyn InputHighlighter>
        })
    })
}

/// The markdown live editor's syntax engine: maps the core parser's spans onto
/// editor text styles, fading source markers on lines the caret is not on.
struct MarkdownLiveHighlighter {
    spans: Vec<markdown_live::MarkdownLiveSpan>,
    line_starts: Vec<usize>,
    len: usize,
    /// The editor caret; the host page keeps it current on every keystroke and
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
    /// Task boxes never fade — they are the click target for toggling.
    fn fades(kind: MarkdownLiveKind) -> Option<f32> {
        match kind {
            MarkdownLiveKind::CodeFence
            | MarkdownLiveKind::Marker
            | MarkdownLiveKind::LinkUrl
            | MarkdownLiveKind::HeadingMarker
            | MarkdownLiveKind::ListMarker
            | MarkdownLiveKind::QuoteMarker => Some(MARKER_FADE),
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
