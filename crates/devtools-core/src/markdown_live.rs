//! Pure markdown span parser for the Notes live editor — an Obsidian-style
//! live preview: the document is styled in place while editing, and source
//! markers (emphasis/inline-code delimiters, link brackets and URLs) fade out
//! wherever the cursor is not on their line.
//!
//! The parser is deliberately line-based and heuristic (a CommonMark subset
//! that covers how notes are actually written). It only ever splits at ASCII
//! syntax bytes, so byte ranges never land inside a multi-byte character.

use std::ops::Range;

/// What a span is, and how the editor should paint it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownLiveKind {
    /// Unstyled text.
    Plain,
    /// The `#`/`##`… run of an ATX heading.
    HeadingMarker,
    /// Heading text after the marker.
    HeadingText,
    /// Rendered emphasis content; flags combine when emphasis nests.
    Emphasis { bold: bool, italic: bool, strike: bool },
    /// Source syntax that fades when the cursor is away: `**`, backticks,
    /// link brackets/parens, escapes.
    Marker,
    /// Inline code content (between the backticks).
    InlineCode,
    /// A fenced-code fence line, including the info string.
    CodeFence,
    /// Fenced code body.
    CodeBlock,
    /// A link label (also image alt text).
    LinkText,
    /// A link URL / autolink / bare URL.
    LinkUrl,
    /// The `>` of a blockquote.
    QuoteMarker,
    /// Quoted text (styled base inside a blockquote).
    QuoteText,
    /// A list bullet or ordered marker (`-`, `*`, `+`, `1.`).
    ListMarker,
    /// An open task box `[ ]`.
    TaskOpen,
    /// A completed task box `[x]`.
    TaskDone,
    /// A horizontal rule line.
    HorizontalRule,
}

/// One styled byte range into the parsed document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownLiveSpan {
    pub range: Range<usize>,
    pub kind: MarkdownLiveKind,
}

/// Parse a markdown document into ordered, non-overlapping spans that cover
/// every byte of `text`. Adjacent same-kind spans (including across newlines,
/// e.g. code-block lines) are merged.
pub fn parse(text: &str) -> Vec<MarkdownLiveSpan> {
    let mut spans: Vec<MarkdownLiveSpan> = Vec::new();
    let mut fence: Option<(u8, usize)> = None; // (fence char, run length)
    let mut pos = 0usize;

    for line in text.split_inclusive('\n') {
        let line_end = pos + line.len();
        let body = line.trim_end_matches(['\r', '\n']);
        let body_end = pos + body.len();
        parse_line(line, body_end, pos, &mut fence, &mut spans);
        // newline not covered by a block span (fence lines cover theirs)
        if spans.last().is_none_or(|s| s.range.end < line_end) {
            spans.push(MarkdownLiveSpan {
                range: body_end..line_end,
                kind: MarkdownLiveKind::Plain,
            });
        }
        pos = line_end;
    }

    // merge adjacent same-kind spans (long code blocks become one run)
    let mut merged: Vec<MarkdownLiveSpan> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(last) = merged.last_mut()
            && last.kind == span.kind
            && last.range.end == span.range.start
        {
            last.range.end = span.range.end;
        } else {
            merged.push(span);
        }
    }
    merged
}

fn push(spans: &mut Vec<MarkdownLiveSpan>, start: usize, end: usize, kind: MarkdownLiveKind) {
    if start < end {
        spans.push(MarkdownLiveSpan { range: start..end, kind });
    }
}

fn parse_line(
    line: &str,
    body_end: usize,
    offset: usize,
    fence: &mut Option<(u8, usize)>,
    spans: &mut Vec<MarkdownLiveSpan>,
) {
    let body = &line[..body_end - offset];
    let indent = body.bytes().take_while(|&b| b == b' ').count();
    let (t, t_off) = if indent <= 3 { (&body[indent..], offset + indent) } else { (body, offset) };
    let body_len = body.len();

    // inside a fence: body lines, or the closing fence (the newline belongs
    // to the block so consecutive lines merge)
    if let Some((ch, len)) = *fence {
        let run = byte_run(t, 0, ch);
        let kind = if run >= len && t[run..].trim().is_empty() {
            *fence = None;
            MarkdownLiveKind::CodeFence
        } else {
            MarkdownLiveKind::CodeBlock
        };
        push(spans, offset, offset + line.len(), kind);
        return;
    }

    // opening fence: ``` or ~~~ (3+) with an info string allowed
    let fence_run = t
        .strip_prefix("```")
        .map(|_| (b'`', byte_run(t, 0, b'`')))
        .or_else(|| t.strip_prefix("~~~").map(|_| (b'~', byte_run(t, 0, b'~'))));
    if let Some((ch, run)) = fence_run
        && run >= 3
    {
        *fence = Some((ch, run));
        push(spans, offset, offset + line.len(), MarkdownLiveKind::CodeFence);
        return;
    }

    // horizontal rule: 3+ of -, _ or * with only spaces between
    if is_horizontal_rule(t) {
        push(spans, offset, offset + body_len, MarkdownLiveKind::HorizontalRule);
        return;
    }

    // ATX heading: 1-6 # then space or end of line
    let hashes = t.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&hashes)
        && (t.len() == hashes || t.as_bytes()[hashes] == b' ' || t.as_bytes()[hashes] == b'\t')
    {
        push(spans, offset, t_off, MarkdownLiveKind::Plain);
        push(spans, t_off, t_off + hashes, MarkdownLiveKind::HeadingMarker);
        push(spans, t_off + hashes, offset + body_len, MarkdownLiveKind::HeadingText);
        return;
    }

    // blockquote: '>' marker, quoted content parsed as inlines
    if t.starts_with('>') {
        push(spans, offset, t_off, MarkdownLiveKind::Plain);
        push(spans, t_off, t_off + 1, MarkdownLiveKind::QuoteMarker);
        parse_inlines(&body[t_off + 1 - offset..], t_off + 1, MarkdownLiveKind::QuoteText, spans);
        return;
    }

    // lists: bullet (-, *, +) or ordered (N. / N)) followed by whitespace
    let marker_len = list_marker_len(t);
    if let Some(marker_len) = marker_len {
        push(spans, offset, t_off, MarkdownLiveKind::Plain);
        push(spans, t_off, t_off + marker_len, MarkdownLiveKind::ListMarker);
        let after_marker = &t[marker_len..];
        // the one separator space/tab after the marker stays plain
        let sep = usize::from(matches!(after_marker.as_bytes().first(), Some(b' ') | Some(b'\t')));
        push(spans, t_off + marker_len, t_off + marker_len + sep, MarkdownLiveKind::Plain);
        let rest_off = t_off + marker_len + sep;
        let rest = &after_marker[sep..];
        // task list item: [ ] or [x/X] after the marker
        let task = rest.strip_prefix("[ ] ").or_else(|| rest.strip_prefix("[ ]\t"));
        let done = task.is_none()
            .then(|| rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")))
            .flatten();
        if let Some(after) = task {
            push(spans, rest_off, rest_off + 3, MarkdownLiveKind::TaskOpen);
            push(spans, rest_off + 3, rest_off + 4, MarkdownLiveKind::Plain);
            parse_inlines(after, rest_off + 4, MarkdownLiveKind::Plain, spans);
        } else if let Some(after) = done {
            push(spans, rest_off, rest_off + 3, MarkdownLiveKind::TaskDone);
            push(spans, rest_off + 3, rest_off + 4, MarkdownLiveKind::Plain);
            // a done task strikes through its whole text, Obsidian-style
            push(spans, rest_off + 4, rest_off + 4 + after.len(), MarkdownLiveKind::Emphasis {
                bold: false,
                italic: false,
                strike: true,
            });
        } else {
            parse_inlines(rest, rest_off, MarkdownLiveKind::Plain, spans);
        }
        return;
    }

    parse_inlines(body, offset, MarkdownLiveKind::Plain, spans);
}

/// Length of a list marker at the start of `t`, if any (bullet or ordered).
fn list_marker_len(t: &str) -> Option<usize> {
    let bytes = t.as_bytes();
    let after_space = |i: usize| bytes.get(i).is_some_and(|&b| b == b' ' || b == b'\t');
    match bytes.first() {
        Some(b'-' | b'+' | b'*') if after_space(1) => Some(1),
        _ => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if (1..=9).contains(&digits)
                && matches!(bytes.get(digits), Some(b'.') | Some(b')'))
                && after_space(digits + 1)
            {
                Some(digits + 1)
            } else {
                None
            }
        }
    }
}

fn is_horizontal_rule(t: &str) -> bool {
    let Some(first) = t.bytes().next() else { return false };
    if !matches!(first, b'-' | b'_' | b'*') {
        return false;
    }
    let mut count = 0;
    for &b in t.as_bytes() {
        match b {
            b if b == first => count += 1,
            b' ' | b'\t' => {}
            _ => return false,
        }
    }
    count >= 3
}

fn byte_run(s: &str, from: usize, ch: u8) -> usize {
    s.as_bytes()[from..].iter().take_while(|&&b| b == ch).count()
}

// ---- inline parsing -----------------------------------------------------------

/// One emphasis pair: marker ranges plus the content between them.
#[derive(PartialEq)]
struct EmphPair {
    open: Range<usize>,
    content: Range<usize>,
    close: Range<usize>,
    bold: bool,
    italic: bool,
    strike: bool,
}

/// Parse inline constructs (relative `s` at absolute `offset`) into `spans`.
/// `base` is the kind unstyled text takes (Plain, or QuoteText in quotes).
fn parse_inlines(s: &str, offset: usize, base: MarkdownLiveKind, spans: &mut Vec<MarkdownLiveSpan>) {
    let bytes = s.as_bytes();
    // Phase A: atomic constructs — code spans, links, escapes, URLs.
    let mut atomic: Vec<(Range<usize>, MarkdownLiveKind)> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'`' => {
                let run = byte_run(s, i, b'`');
                if let Some(j) = find_backtick_run(s, i + run, run) {
                    atomic.push((i..i + run, MarkdownLiveKind::Marker));
                    atomic.push((i + run..j, MarkdownLiveKind::InlineCode));
                    atomic.push((j..j + run, MarkdownLiveKind::Marker));
                    i = j + run;
                } else {
                    i += run;
                }
            }
            b'\\' if bytes.get(i + 1).is_some_and(|b| b.is_ascii_punctuation()) => {
                atomic.push((i..i + 2, MarkdownLiveKind::Marker));
                i += 2;
            }
            b'!' | b'[' => {
                if let Some(link) = parse_link(s, i) {
                    atomic.push((link.marker, MarkdownLiveKind::Marker));
                    atomic.push((link.label, MarkdownLiveKind::LinkText));
                    atomic.push((link.mid, MarkdownLiveKind::Marker));
                    atomic.push((link.url, MarkdownLiveKind::LinkUrl));
                    atomic.push((link.close.clone(), MarkdownLiveKind::Marker));
                    i = link.close.end;
                } else {
                    i += 1;
                }
            }
            b'<' => {
                let end = s[i + 1..].find('>').map(|k| i + 1 + k);
                if let Some(end) = end
                    && s[i..end].contains("://")
                {
                    atomic.push((i..end + 1, MarkdownLiveKind::LinkUrl));
                    i = end + 1;
                } else {
                    i += 1;
                }
            }
            b':' if s[i..].starts_with("://") && is_bare_url_start(s, i) => {
                let start = bare_url_start(s, i);
                let end = bare_url_end(s, i);
                atomic.push((start..end, MarkdownLiveKind::LinkUrl));
                i = end;
            }
            _ => i += 1,
        }
    }

    // Phase B: emphasis over the gaps between the atomic spans. Delimiters
    // pair across the whole line (e.g. bold around an inline-code span), but
    // only ever live in gaps, never inside an atomic span.
    let mut covered: Vec<(Range<usize>, MarkdownLiveKind)> = atomic;
    let mut cursor = 0usize;
    let mut gaps: Vec<Range<usize>> = Vec::new();
    for (range, _) in &covered {
        if range.start > cursor {
            gaps.push(cursor..range.start);
        }
        cursor = range.end;
    }
    if cursor < s.len() {
        gaps.push(cursor..s.len());
    }
    let pairs = resolve_emphasis(s, &gaps);
    let mut runs: Vec<(Range<usize>, MarkdownLiveKind)> = Vec::new();
    flatten_pairs(
        &pairs,
        0..s.len(),
        base,
        &gaps,
        &mut runs,
    );
    covered.extend(runs);

    covered.sort_by_key(|(range, _)| range.start);
    covered.dedup_by_key(|(range, _)| range.clone());
    cursor = 0;
    for (range, kind) in covered {
        if range.start > cursor {
            push(spans, offset + cursor, offset + range.start, base);
        }
        push(spans, offset + range.start, offset + range.end, kind);
        cursor = range.end;
    }
    if cursor < s.len() {
        push(spans, offset + cursor, offset + s.len(), base);
    }
}

/// A `[label](url)` / `![alt](url)` starting at `i`, as its five relative
/// ranges: (marker, label, "](", url, ")").
struct LinkParts {
    marker: Range<usize>,
    label: Range<usize>,
    mid: Range<usize>,
    url: Range<usize>,
    close: Range<usize>,
}

fn parse_link(s: &str, i: usize) -> Option<LinkParts> {
    let bytes = s.as_bytes();
    let (start, lb) = if bytes[i] == b'!' { (i, i + 1) } else { (i, i) };
    if bytes.get(lb) != Some(&b'[') {
        return None;
    }
    // label: up to the next ']' without another '[' in between
    let mut j = lb + 1;
    while j < bytes.len() && bytes[j] != b']' && bytes[j] != b'[' {
        j += 1;
    }
    if j >= bytes.len() || bytes[j] != b']' || bytes.get(j + 1) != Some(&b'(') {
        return None;
    }
    let url_start = j + 2;
    let k = s[url_start..].find(')').map(|rel| url_start + rel)?;
    Some(LinkParts {
        marker: start..lb + 1,
        label: lb + 1..j,
        mid: j..j + 2,
        url: url_start..k,
        close: k..k + 1,
    })
}

/// The position of a backtick run of exactly `len` ticks, starting at `from`.
fn find_backtick_run(s: &str, from: usize, len: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let run = byte_run(s, i, b'`');
            if run == len {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

/// `://` at `i` opens a bare URL only when preceded by a scheme
/// (at least "http." style letters and a valid scheme char before us).
fn is_bare_url_start(s: &str, i: usize) -> bool {
    let bytes = s.as_bytes();
    let mut k = i;
    while k > 0
        && bytes[k - 1].is_ascii_alphanumeric()
    {
        k -= 1;
    }
    // require at least one letter and a scheme-ish separator boundary
    k < i && bytes[k].is_ascii_alphabetic() && (k == 0 || !bytes[k - 1].is_ascii_alphanumeric())
}

fn bare_url_start(s: &str, colon: usize) -> usize {
    let bytes = s.as_bytes();
    let mut k = colon;
    while k > 0 && bytes[k - 1].is_ascii_alphanumeric() {
        k -= 1;
    }
    k
}

fn bare_url_end(s: &str, colon: usize) -> usize {
    let bytes = s.as_bytes();
    let mut k = colon;
    while k < bytes.len() && !matches!(bytes[k], b' ' | b'\t' | b'<' | b'>') {
        k += 1;
    }
    k
}

/// Resolve emphasis delimiter runs in `gaps` into properly-nested pairs.
fn resolve_emphasis(s: &str, gaps: &[Range<usize>]) -> Vec<EmphPair> {
    let bytes = s.as_bytes();
    // collect delimiter runs within the gaps
    struct Delim {
        pos: usize,
        len: usize,
        ch: u8,
        can_open: bool,
        can_close: bool,
    }
    let mut delims: Vec<Delim> = Vec::new();
    for gap in gaps {
        let mut i = gap.start;
        while i < gap.end {
            let b = bytes[i];
            if matches!(b, b'*' | b'_' | b'~') {
                let end = (i + byte_run(s, i, b)).min(gap.end);
                let len = end - i;
                let prev = char_before(bytes, i);
                let next = char_after(bytes, end);
                let left = left_flanking(prev, next);
                let right = right_flanking(prev, next);
                let (can_open, can_close) = match b {
                    b'~' if len == 2 => (left, right),
                    b'~' => (false, false),
                    b'_' => {
                        let intraword = prev.is_some_and(|c| c.is_alphanumeric())
                            && next.is_some_and(|c| c.is_alphanumeric());
                        (!intraword && left && !right, !intraword && right && !left)
                    }
                    _ => (left, right),
                };
                delims.push(Delim { pos: i, len, ch: b, can_open, can_close });
                i = end;
            } else {
                i += 1;
            }
        }
    }

    // pair closers with the nearest same-char same-length opener below
    let mut open: Vec<usize> = Vec::new(); // stack of delims indices
    let mut pairs: Vec<EmphPair> = Vec::new();
    for (idx, delim) in delims.iter().enumerate() {
        let mut matched = false;
        if delim.can_close {
            for stack_idx in (0..open.len()).rev() {
                let cand = &delims[open[stack_idx]];
                if cand.ch == delim.ch
                    && cand.len == delim.len
                    && cand.can_open
                    && cand.pos + cand.len <= delim.pos
                {
                    let (bold, italic, strike) = match (delim.ch, delim.len) {
                        (b'~', 2) => (false, false, true),
                        (_, 3..) => (true, true, false),
                        (_, 2) => (true, false, false),
                        _ => (false, true, false),
                    };
                    pairs.push(EmphPair {
                        open: cand.pos..cand.pos + cand.len,
                        content: cand.pos + cand.len..delim.pos,
                        close: delim.pos..delim.pos + delim.len,
                        bold,
                        italic,
                        strike,
                    });
                    open.remove(stack_idx);
                    matched = true;
                    break;
                }
            }
        }
        if !matched && delim.can_open {
            open.push(idx);
        }
    }

    // keep only properly nested pairs: sort by content start (outer first),
    // drop any pair that partially overlaps an accepted one
    pairs.sort_by(|a, b| a.content.start.cmp(&b.content.start).then(b.content.end.cmp(&a.content.end)));
    let mut accepted: Vec<EmphPair> = Vec::new();
    let mut stack: Vec<Range<usize>> = Vec::new();
    for pair in pairs {
        while let Some(top) = stack.last() {
            if pair.content.start >= top.end {
                stack.pop();
            } else {
                break;
            }
        }
        if let Some(top) = stack.last()
            && pair.content.end > top.end
        {
            continue; // partial overlap
        }
        stack.push(pair.content.clone());
        accepted.push(pair);
    }
    accepted
}

fn char_before(bytes: &[u8], i: usize) -> Option<char> {
    if i == 0 { None } else { Some(bytes[i - 1] as char) }
}

fn char_after(bytes: &[u8], i: usize) -> Option<char> {
    bytes.get(i).map(|&b| b as char)
}

/// CommonMark-ish left-flanking: may open emphasis.
fn left_flanking(prev: Option<char>, next: Option<char>) -> bool {
    next.is_some_and(|n| !n.is_whitespace())
        && (next.is_none_or(|n| !n.is_ascii_punctuation())
            || prev.is_none_or(|p| p.is_whitespace() || p.is_ascii_punctuation()))
}

/// CommonMark-ish right-flanking: may close emphasis.
fn right_flanking(prev: Option<char>, next: Option<char>) -> bool {
    prev.is_some_and(|p| !p.is_whitespace())
        && (prev.is_none_or(|p| !p.is_ascii_punctuation())
            || next.is_none_or(|n| n.is_whitespace() || n.is_ascii_punctuation()))
}

/// Emit the runs of an emphasis forest: segments between nested pairs take
/// their enclosing pair's emphasis; markers take Marker. Segments only ever
/// cover the non-atomic gaps, so atomic spans (code/links) stay intact.
fn flatten_pairs(
    pairs: &[EmphPair],
    region: Range<usize>,
    base: MarkdownLiveKind,
    gaps: &[Range<usize>],
    runs: &mut Vec<(Range<usize>, MarkdownLiveKind)>,
) {
    // children of this region: pairs inside it that are not contained in
    // another pair inside it (their own recursion handles the nested ones)
    let in_region = |p: &EmphPair| p.open.start >= region.start && p.close.end <= region.end;
    let mut children: Vec<&EmphPair> = pairs
        .iter()
        .filter(|p| {
            in_region(p)
                && !pairs
                    .iter()
                    .any(|q| q != *p && in_region(q)
                        && q.content.start <= p.content.start
                        && p.content.end <= q.content.end)
        })
        .collect();
    children.sort_by_key(|p| p.open.start);

    let mut cursor = region.start;
    let segment = |start: usize, end: usize, kind: MarkdownLiveKind, runs: &mut Vec<_>| {
        for gap in gaps {
            let from = gap.start.max(start);
            let to = gap.end.min(end);
            if from < to {
                runs.push((from..to, kind));
            }
        }
    };
    for pair in children {
        segment(cursor, pair.open.start, base, runs);
        runs.push((pair.open.clone(), MarkdownLiveKind::Marker));
        let nested_kind = MarkdownLiveKind::Emphasis {
            bold: pair.bold,
            italic: pair.italic,
            strike: pair.strike,
        };
        flatten_pairs(pairs, pair.content.clone(), nested_kind, gaps, runs);
        runs.push((pair.close.clone(), MarkdownLiveKind::Marker));
        cursor = pair.close.end;
    }
    segment(cursor, region.end, base, runs);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind_at(text: &str, byte: usize) -> MarkdownLiveKind {
        parse(text)
            .into_iter()
            .find(|s| s.range.contains(&byte))
            .map(|s| s.kind)
            .unwrap_or_else(|| panic!("no span covers byte {byte} of {text:?}"))
    }

    /// spans must be ordered, non-overlapping, and cover every byte
    fn assert_covered(text: &str) {
        let spans = parse(text);
        let mut cursor = 0;
        for span in &spans {
            assert_eq!(span.range.start, cursor, "gap/overlap at {} in {text:?}", span.range.start);
            cursor = span.range.end;
        }
        assert_eq!(cursor, text.len(), "coverage must reach end in {text:?}");
    }

    const BOLD: MarkdownLiveKind = MarkdownLiveKind::Emphasis { bold: true, italic: false, strike: false };
    const ITALIC: MarkdownLiveKind = MarkdownLiveKind::Emphasis { bold: false, italic: true, strike: false };
    const STRIKE: MarkdownLiveKind = MarkdownLiveKind::Emphasis { bold: false, italic: false, strike: true };

    #[test]
    fn headings() {
        let text = "## Title\nbody";
        assert_eq!(kind_at(text, 0), MarkdownLiveKind::HeadingMarker);
        assert_eq!(kind_at(text, 1), MarkdownLiveKind::HeadingMarker);
        assert_eq!(kind_at(text, 3), MarkdownLiveKind::HeadingText);
        assert_eq!(kind_at(text, 9), MarkdownLiveKind::Plain);
        assert_covered(text);
        // hashes without a space are plain text
        assert_eq!(kind_at("#hashtag", 0), MarkdownLiveKind::Plain);
        // seven hashes is not a heading
        assert_eq!(kind_at("####### x", 0), MarkdownLiveKind::Plain);
    }

    #[test]
    fn emphasis_pairs() {
        assert_eq!(kind_at("**bold**", 2), BOLD);
        assert_eq!(kind_at("__bold__", 2), BOLD);
        assert_eq!(kind_at("*ital*", 1), ITALIC);
        assert_eq!(kind_at("***both***", 3), MarkdownLiveKind::Emphasis { bold: true, italic: true, strike: false });
        assert_eq!(kind_at("~~gone~~", 2), STRIKE);
        // markers themselves
        assert_eq!(kind_at("**bold**", 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at("**bold**", 6), MarkdownLiveKind::Marker);
        // nesting: bold containing italic
        let text = "**a *b* c**";
        assert_eq!(kind_at(text, 2), BOLD);
        assert_eq!(kind_at(text, 3), BOLD);
        assert_eq!(kind_at(text, 5), ITALIC);
        assert_eq!(kind_at(text, 4), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 8), BOLD);
        // unclosed stays plain
        assert_eq!(kind_at("**bold", 0), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("a * b", 2), MarkdownLiveKind::Plain);
        // mismatched lengths stay plain
        assert_eq!(kind_at("**bold*", 0), MarkdownLiveKind::Plain);
        // intraword underscores (snake_case) stay plain
        assert_eq!(kind_at("snake_case_name", 5), MarkdownLiveKind::Plain);
        // intraword asterisks are emphasis
        assert_eq!(kind_at("a*b*c", 2), ITALIC);
        assert_covered("**a *b* c**");
        assert_covered("***both*** and ~~gone~~");
    }

    #[test]
    fn emphasis_does_not_span_lines() {
        let text = "*one\ntwo*";
        assert_eq!(kind_at(text, 1), MarkdownLiveKind::Plain);
        assert_eq!(kind_at(text, 5), MarkdownLiveKind::Plain);
    }

    #[test]
    fn inline_code() {
        assert_eq!(kind_at("`code`", 1), MarkdownLiveKind::InlineCode);
        assert_eq!(kind_at("`code`", 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at("``a ` b``", 2), MarkdownLiveKind::InlineCode);
        // unmatched backticks are plain
        assert_eq!(kind_at("`oops", 0), MarkdownLiveKind::Plain);
        assert_covered("x `code` y");
    }

    #[test]
    fn links_and_urls() {
        let text = "see [docs](https://example.com) now";
        assert_eq!(kind_at(text, 4), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 5), MarkdownLiveKind::LinkText);
        assert_eq!(kind_at(text, 10), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 12), MarkdownLiveKind::LinkUrl);
        assert_eq!(kind_at(text, 30), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(text, 32), MarkdownLiveKind::Plain);
        assert_covered(text);

        let img = "![alt](pic.png)";
        assert_eq!(kind_at(img, 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(img, 2), MarkdownLiveKind::LinkText);
        assert_eq!(kind_at(img, 7), MarkdownLiveKind::LinkUrl);

        let auto = "<https://example.com>";
        assert_eq!(kind_at(auto, 1), MarkdownLiveKind::LinkUrl);

        let bare = "go https://example.com/x end";
        assert_eq!(kind_at(bare, 3), MarkdownLiveKind::LinkUrl);

        // no url part: plain text
        assert_eq!(kind_at("[note]", 0), MarkdownLiveKind::Plain);
    }

    #[test]
    fn quotes_and_lists() {
        assert_eq!(kind_at("> quoted", 0), MarkdownLiveKind::QuoteMarker);
        assert_eq!(kind_at("> quoted", 2), MarkdownLiveKind::QuoteText);
        // markdown inside quotes still styles
        assert_eq!(kind_at("> **b**", 4), BOLD);

        assert_eq!(kind_at("- item", 0), MarkdownLiveKind::ListMarker);
        assert_eq!(kind_at("- item", 2), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("3. item", 0), MarkdownLiveKind::ListMarker);
        assert_eq!(kind_at("1) item", 0), MarkdownLiveKind::ListMarker);

        assert_eq!(kind_at("- [ ] todo", 2), MarkdownLiveKind::TaskOpen);
        assert_eq!(kind_at("- [ ] todo", 6), MarkdownLiveKind::Plain);
        assert_eq!(kind_at("- [x] done", 2), MarkdownLiveKind::TaskDone);
        assert_eq!(kind_at("- [x] done", 6), STRIKE);
        assert_covered("- [x] done thing");
    }

    #[test]
    fn fenced_code() {
        let text = "```rust\nfn x() {}\n```\nafter";
        assert_eq!(kind_at(text, 0), MarkdownLiveKind::CodeFence);
        assert_eq!(kind_at(text, 8), MarkdownLiveKind::CodeBlock);
        assert_eq!(kind_at(text, 20), MarkdownLiveKind::CodeFence);
        assert_eq!(kind_at(text, 24), MarkdownLiveKind::Plain);
        assert_covered(text);

        // unterminated fence: body until EOF
        let open = "```\nbody";
        assert_eq!(kind_at(open, 5), MarkdownLiveKind::CodeBlock);

        // no emphasis inside code
        let code = "```\n**not bold**\n```";
        assert_eq!(kind_at(code, 5), MarkdownLiveKind::CodeBlock);
    }

    #[test]
    fn rules_and_escapes() {
        assert_eq!(kind_at("---", 0), MarkdownLiveKind::HorizontalRule);
        assert_eq!(kind_at("*  *  *", 0), MarkdownLiveKind::HorizontalRule);
        assert_eq!(kind_at("____", 0), MarkdownLiveKind::HorizontalRule);
        // escaped marker fades as source
        assert_eq!(kind_at(r"\*not\*", 0), MarkdownLiveKind::Marker);
        assert_eq!(kind_at(r"\*not\*", 2), MarkdownLiveKind::Plain);
        assert_covered(r"\*escaped\* rest");
    }

    #[test]
    fn adjacent_same_kind_merge() {
        let text = "```\na\nb\n```";
        let spans = parse(text);
        // fence, one merged code-block run, fence — nothing between a and b
        assert_eq!(spans.len(), 3, "{spans:?}");
        assert_covered(text);
    }

    #[test]
    fn empty_and_plain() {
        assert!(parse("").is_empty());
        assert_covered("just text, nothing markdown");
        assert_covered("");
        assert_covered("multi\nline\nplain");
    }

    #[test]
    fn unicode_never_splits_a_char() {
        let text = "**ğünaydın** dünya";
        assert_eq!(kind_at(text, 2), BOLD);
        assert_covered(text);
        // combining chars and CJK content
        let emoji = "- [x] 🎉 ship it";
        assert_eq!(kind_at(emoji, 2), MarkdownLiveKind::TaskDone);
        assert_covered(emoji);
    }
}
