//! The unified diff renderer: Zed-style rows — one muted line-number gutter
//! (old number on removed rows, new number on added/context rows), a colored
//! sign column and full-width add/del bands, with a lightweight per-language
//! syntax pass over the code. Rows are separate divs, so text selection does
//! not span lines.

use crate::theme;
use gpui::prelude::*;
use gpui::{
    div, px, AbsoluteLength, AnyElement, FontStyle, HighlightStyle, Hsla, SharedString, StyledText,
    Window,
};
use std::ops::Range;

/// Renders a unified diff. Callers store the patch with `\r` already
/// stripped (one pass at load), so this never rewrites the text per render.
pub fn render_diff(text: &str, font_detail: f32, window: &mut Window) -> AnyElement {
    let text = text.trim_end_matches('\n');
    if text.is_empty() {
        return div().into_any_element();
    }
    let base_style = window.text_style();
    let mut style = base_style.clone();
    style.font_family = theme::code_font().into();
    style.font_size = AbsoluteLength::Pixels(px(font_detail));

    let gutter_w = px(font_detail * 2.7);
    let sign_w = px(font_detail * 1.4);

    let row = |number: Option<usize>,
               sign: Option<(&'static str, gpui::Rgba)>,
               band: Option<Hsla>,
               text_color: gpui::Rgba,
               code: &str,
               spans: Vec<(Range<usize>, Tok)>| {
        let mut r = div().w_full().flex().flex_row().px_2().text_color(text_color);
        if let Some(band) = band {
            r = r.bg(band);
        }
        r = match number {
            Some(n) => r.child(
                div()
                    .w(gutter_w)
                    .flex_none()
                    .flex()
                    .justify_end()
                    .text_color(theme::DIFF_GUTTER)
                    .child(n.to_string()),
            ),
            None => r.child(div().w(gutter_w + sign_w).flex_none()),
        };
        r = match sign {
            Some((glyph, color)) => r.child(
                div()
                    .w(sign_w)
                    .flex_none()
                    .text_color(color)
                    .child(glyph.to_string()),
            ),
            None => r.child(div().w(sign_w).flex_none()),
        };
        if spans.is_empty() {
            let code = if code.is_empty() { " " } else { code };
            r.child(div().flex_1().min_w_0().child(code.to_string())).into_any_element()
        } else {
            let highlights =
                spans.into_iter().map(|(range, tok)| (range, tok_style(tok))).collect::<Vec<_>>();
            r.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(StyledText::new(code.to_string()).with_default_highlights(&style, highlights)),
            )
            .into_any_element()
        }
    };

    let mut rows: Vec<AnyElement> = Vec::new();
    if !text.contains("@@") && !text.contains("diff --git") {
        rows.push(row(None, None, None, theme::TEXT_MUTED, text, Vec::new()));
        return div()
            .w_full()
            .flex()
            .flex_col()
            .font_family(SharedString::from(theme::code_font()))
            .text_size(px(font_detail))
            .children(rows)
            .into_any_element();
    }

    let (mut old, mut new) = (0usize, 0usize);
    let mut lang = Lang::Plain;
    let mut seen_hunk = false;
    let mut syn = SynState::default();
    for raw in text.split('\n') {
        if let Some((o, n)) = hunk_header(raw) {
            old = o;
            new = n;
            seen_hunk = true;
            rows.push(row(None, None, Some(theme::DIFF_HUNK_BG), theme::TEXT_MUTED, raw, Vec::new()));
            continue;
        }
        if raw.starts_with('\\') || (!seen_hunk && is_meta(raw)) {
            if raw.starts_with("diff --git") {
                seen_hunk = false;
                lang = Lang::Plain;
            }
            if let Some(path) = raw.strip_prefix("+++ ") {
                lang = Lang::from_path(path.split('\t').next().unwrap_or(""));
            }
            rows.push(row(None, None, None, theme::TEXT_FAINT, raw, Vec::new()));
            continue;
        }
        if let Some(rest) = raw.strip_prefix('+') {
            let spans = highlight(rest, lang, &mut syn);
            rows.push(row(
                Some(new),
                Some(("+", theme::SUCCESS_BRIGHT)),
                Some(theme::DIFF_ADD_BG),
                theme::TEXT_DIM,
                rest,
                spans,
            ));
            new += 1;
        } else if let Some(rest) = raw.strip_prefix('-') {
            let spans = highlight(rest, lang, &mut syn);
            rows.push(row(
                Some(old),
                Some(("-", theme::DANGER)),
                Some(theme::DIFF_DEL_BG),
                theme::TEXT_DIM,
                rest,
                spans,
            ));
            old += 1;
        } else {
            let spans = highlight(raw, lang, &mut syn);
            rows.push(row(Some(old), None, None, theme::TEXT_DIM, raw, spans));
            old += 1;
            new += 1;
        }
    }

    // GPUI shapes a StyledText at the *ambient* text style, not the highlight
    // base style: the code font and the diff size must ride on the container,
    // otherwise the rows ignore font_detail and wrapped segments render in the
    // UI font.
    div()
        .w_full()
        .flex()
        .flex_col()
        .font_family(SharedString::from(theme::code_font()))
        .text_size(px(font_detail))
        .children(rows)
        .into_any_element()
}

fn tok_style(tok: Tok) -> HighlightStyle {
    let (color, italic) = match tok {
        Tok::Comment => (theme::TEXT_FAINT, true),
        Tok::Str => (theme::SUCCESS_BRIGHT, false),
        Tok::Number | Tok::Const => (theme::WARN, false),
        Tok::Keyword => (theme::PURPLE_BRIGHT, false),
        Tok::Type => (theme::AMBER_BRIGHT, false),
        Tok::Func => (theme::BLUE_BRIGHT, false),
        Tok::Attr => (theme::GOLD_BRIGHT, false),
    };
    HighlightStyle {
        color: Some(color.into()),
        font_style: Some(if italic { FontStyle::Italic } else { FontStyle::Normal }),
        ..Default::default()
    }
}

fn hunk_header(line: &str) -> Option<(usize, usize)> {
    // @@ -old[,n] +new[,n] @@
    let rest = line.strip_prefix("@@ -")?;
    let plus = rest.find(" +")?;
    let old: usize = rest[..plus].split(',').next()?.trim().parse().ok()?;
    let tail = &rest[plus + 2..];
    let end = tail.find(" @@")?;
    let new: usize = tail[..end].split(',').next()?.trim().parse().ok()?;
    Some((old, new))
}

fn is_meta(line: &str) -> bool {
    ["diff --git", "index ", "--- ", "+++ ", "new file", "deleted file", "old mode", "new mode", "similarity", "rename ", "Binary files", "GIT binary patch"]
        .iter()
        .any(|p| line.starts_with(p))
}

// ---- syntax pass ------------------------------------------------------------

#[derive(Clone, Copy)]
enum Tok {
    Comment,
    Str,
    Number,
    Keyword,
    Type,
    Const,
    Func,
    Attr,
}

#[derive(Default)]
struct SynState {
    block: bool,
    quote: Option<char>,
    quote_lines: u8,
}

fn highlight(line: &str, lang: Lang, st: &mut SynState) -> Vec<(Range<usize>, Tok)> {
    let mut spans: Vec<(Range<usize>, Tok)> = Vec::new();
    let bytes = line.as_bytes();
    let end = line.len();
    let mut i = 0;
    while i < end {
        if st.block {
            match line[i..].find("*/") {
                Some(off) => {
                    spans.push((i..i + off + 2, Tok::Comment));
                    st.block = false;
                    i += off + 2;
                }
                None => {
                    spans.push((i..end, Tok::Comment));
                    i = end;
                }
            }
            continue;
        }
        if let Some(q) = st.quote {
            st.quote_lines += 1;
            let (j, closed) = scan_string(line, i, q);
            spans.push((i..j, Tok::Str));
            st.quote = if closed || st.quote_lines > 2 { None } else { Some(q) };
            i = j;
            continue;
        }
        let c = line[i..].chars().next().unwrap();
        let rest = &line[i..];
        if c == '/' && lang.c_comments() {
            if rest.starts_with("//") {
                spans.push((i..end, Tok::Comment));
                break;
            }
            if rest.starts_with("/*") {
                spans.push((i..i + 2, Tok::Comment));
                st.block = true;
                i += 2;
                continue;
            }
        }
        if c == '#' && lang.hash_comment() {
            spans.push((i..end, Tok::Comment));
            break;
        }
        if c == '#' && lang == Lang::Rust && (rest.starts_with("#[") || rest.starts_with("#![")) {
            let mut j = i + if rest.starts_with("#![") { 3 } else { 2 };
            while let Some(nc) = line.get(j..).and_then(|s| s.chars().next()) {
                if nc.is_alphanumeric() || nc == '_' || nc == ':' {
                    j += nc.len_utf8();
                } else {
                    break;
                }
            }
            spans.push((i..j, Tok::Attr));
            i = j;
            continue;
        }
        if c == '"' || c == '`' {
            let (j, closed) = scan_string(line, i + 1, c);
            spans.push((i..j, Tok::Str));
            st.quote = if closed { None } else { Some(c) };
            st.quote_lines = 0;
            i = j;
            continue;
        }
        if c == '\'' {
            if lang == Lang::Rust && !rest[1..].starts_with('\\') {
                // `'x'` = char literal, `'a` = lifetime: a closing quote within
                // 3 content chars decides
                let close = line
                    .get(i + 2..)
                    .and_then(|s| s.find('\''))
                    .map(|k| i + 2 + k)
                    .filter(|&at| at <= i + 4);
                match close {
                    Some(at) => {
                        spans.push((i..at + 1, Tok::Str));
                        i = at + 1;
                    }
                    None => {
                        let mut j = i + 1;
                        while let Some(nc) = line.get(j..).and_then(|s| s.chars().next()) {
                            if is_ident(nc) {
                                j += nc.len_utf8();
                            } else {
                                break;
                            }
                        }
                        if j > i + 1 {
                            spans.push((i..j, Tok::Attr));
                        }
                        i = j;
                    }
                }
            } else {
                let (j, closed) = scan_string(line, i + 1, '\'');
                spans.push((i..j, Tok::Str));
                st.quote = if closed { None } else { Some('\'') };
                st.quote_lines = 0;
                i = j;
            }
            continue;
        }
        if c == '@'
            && lang != Lang::Plain
            && line.get(i + 1..).and_then(|s| s.chars().next()).is_some_and(is_ident_start)
        {
            let mut j = i + 1;
            while let Some(nc) = line.get(j..).and_then(|s| s.chars().next()) {
                if is_ident(nc) {
                    j += nc.len_utf8();
                } else {
                    break;
                }
            }
            spans.push((i..j, Tok::Attr));
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i + 1;
            while j < end && (bytes[j].is_ascii_digit() || bytes[j] == b'_') {
                j += 1;
            }
            if j + 1 < end && bytes[j] == b'.' && bytes[j + 1].is_ascii_digit() {
                j += 2;
                while j < end && (bytes[j].is_ascii_digit() || bytes[j] == b'_') {
                    j += 1;
                }
            }
            if j < end && (bytes[j] == b'e' || bytes[j] == b'E') {
                let mut k = j + 1;
                if k < end && (bytes[k] == b'+' || bytes[k] == b'-') {
                    k += 1;
                }
                if k < end && bytes[k].is_ascii_digit() {
                    j = k;
                    while j < end && bytes[j].is_ascii_digit() {
                        j += 1;
                    }
                }
            }
            let mut k = j;
            while k < end && bytes[k].is_ascii_alphanumeric() {
                k += 1;
            }
            if k - j <= 3 {
                j = k;
            }
            spans.push((i..j, Tok::Number));
            i = j;
            continue;
        }
        if is_ident_start(c) {
            let mut j = i + c.len_utf8();
            while let Some(nc) = line.get(j..).and_then(|s| s.chars().next()) {
                if is_ident(nc) {
                    j += nc.len_utf8();
                } else {
                    break;
                }
            }
            if let Some(tok) = classify(&line[i..j], lang, &line[j..]) {
                spans.push((i..j, tok));
            }
            i = j;
            continue;
        }
        i += c.len_utf8();
    }
    spans
}

fn scan_string(line: &str, from: usize, quote: char) -> (usize, bool) {
    let bytes = line.as_bytes();
    let mut j = from;
    while j < line.len() {
        match bytes[j] {
            b'\\' => j = (j + 2).min(line.len()),
            b if b as char == quote => return (j + 1, true),
            _ => j += 1,
        }
    }
    (line.len(), false)
}

fn classify(word: &str, lang: Lang, after: &str) -> Option<Tok> {
    if lang.keywords().contains(&word) {
        return Some(Tok::Keyword);
    }
    if CONSTS.contains(&word) {
        return Some(Tok::Const);
    }
    let next = after.trim_start().chars().next();
    if lang == Lang::Rust && next == Some('!') {
        return Some(Tok::Attr);
    }
    if next == Some('(') {
        return Some(Tok::Func);
    }
    if word.chars().next().is_some_and(char::is_uppercase) {
        return Some(Tok::Type);
    }
    if word.len() >= 2 && word.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') {
        return Some(Tok::Const);
    }
    None
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

const CONSTS: &[&str] = &["FALSE", "NIL", "NULL", "None", "True", "false", "nil", "null", "true", "undefined"];

#[derive(Clone, Copy, PartialEq)]
enum Lang {
    Plain,
    Rust,
    CSharp,
    JsTs,
    Go,
    Python,
    CCpp,
    Shell,
    Yaml,
    Json,
}

impl Lang {
    fn from_path(arg: &str) -> Lang {
        let path = arg.trim().trim_matches('"');
        let path = path.strip_prefix("b/").unwrap_or(path);
        let Some((_, ext)) = path.rsplit_once('.') else { return Lang::Plain };
        match ext.to_lowercase().as_str() {
            "rs" => Lang::Rust,
            "cs" => Lang::CSharp,
            "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "mts" | "cts" => Lang::JsTs,
            "go" => Lang::Go,
            "py" | "pyi" | "pyw" => Lang::Python,
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" | "ipp" => Lang::CCpp,
            "sh" | "bash" | "zsh" | "fish" => Lang::Shell,
            "yml" | "yaml" | "toml" | "ini" | "conf" | "properties" | "cfg" => Lang::Yaml,
            "json" | "jsonc" | "json5" => Lang::Json,
            _ => Lang::Plain,
        }
    }

    fn c_comments(self) -> bool {
        matches!(self, Lang::Rust | Lang::CSharp | Lang::JsTs | Lang::Go | Lang::CCpp | Lang::Json)
    }

    fn hash_comment(self) -> bool {
        matches!(self, Lang::Python | Lang::Shell | Lang::Yaml)
    }

    fn keywords(self) -> &'static [&'static str] {
        match self {
            Lang::Rust => &["as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "type", "union", "unsafe", "use", "where", "while"],
            Lang::CSharp => &["abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char", "checked", "class", "const", "continue", "decimal", "default", "delegate", "do", "double", "else", "enum", "event", "explicit", "extern", "finally", "fixed", "float", "for", "foreach", "get", "global", "goto", "if", "implicit", "in", "init", "int", "interface", "internal", "is", "lock", "long", "namespace", "new", "nullable", "object", "operator", "out", "override", "params", "partial", "private", "protected", "public", "readonly", "record", "ref", "required", "sbyte", "sealed", "set", "short", "sizeof", "stackalloc", "static", "string", "struct", "switch", "this", "throw", "try", "typeof", "uint", "ulong", "unchecked", "unsafe", "ushort", "using", "var", "virtual", "void", "volatile", "when", "where", "while", "yield"],
            Lang::JsTs => &["abstract", "any", "as", "async", "await", "bigint", "boolean", "break", "case", "catch", "class", "const", "continue", "debugger", "declare", "default", "delete", "do", "else", "enum", "export", "extends", "finally", "for", "from", "function", "get", "if", "implements", "import", "in", "infer", "instanceof", "interface", "keyof", "let", "namespace", "never", "new", "null", "number", "of", "object", "package", "private", "protected", "public", "readonly", "return", "set", "static", "string", "super", "switch", "symbol", "this", "throw", "true", "try", "type", "typeof", "undefined", "unknown", "var", "void", "while", "with", "yield"],
            Lang::Go => &["break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range", "return", "select", "struct", "switch", "type", "var"],
            Lang::Python => &["and", "as", "assert", "async", "await", "break", "case", "class", "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "match", "nonlocal", "not", "or", "pass", "raise", "return", "self", "try", "while", "with", "yield"],
            Lang::CCpp => &["alignas", "alignof", "auto", "bool", "break", "case", "catch", "char", "class", "const", "constexpr", "continue", "decltype", "default", "delete", "do", "double", "else", "enum", "explicit", "extern", "false", "float", "for", "friend", "goto", "if", "inline", "int", "long", "mutable", "namespace", "new", "noexcept", "nullptr", "operator", "private", "protected", "public", "register", "return", "short", "signed", "sizeof", "static", "struct", "switch", "template", "this", "throw", "true", "try", "typedef", "typeid", "typename", "union", "unsigned", "using", "virtual", "void", "volatile", "while"],
            Lang::Shell => &["case", "cd", "do", "done", "echo", "elif", "else", "esac", "eval", "exec", "exit", "export", "fi", "for", "function", "if", "in", "local", "return", "set", "shift", "source", "then", "trap", "unset", "while"],
            Lang::Yaml => &["no", "off", "yes"],
            Lang::Json | Lang::Plain => &[],
        }
    }
}
