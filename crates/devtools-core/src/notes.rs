//! The per-repo notes store: one subfolder per repository under the
//! configured root (default ~/.devtools/notes), markdown files only. Tree
//! loading, CRUD with sanitized names, capped content search — plus the pure
//! markdown editing operations the Notes editor's toolbar drives (wrap/toggle
//! semantics).

use crate::paths::AppPaths;
use std::path::{Path, PathBuf};

const SEARCH_CONTENT_CAP_BYTES: u64 = 2 * 1024 * 1024;
const SEARCH_FILE_CAP: usize = 2000;

#[derive(Clone, Debug)]
pub struct NotesTreeItem {
    pub name: String,
    pub full_path: PathBuf,
    pub is_folder: bool,
    pub children: Vec<NotesTreeItem>,
}

#[derive(Clone, Debug)]
pub struct NotesSearchHit {
    pub path: PathBuf,
    pub title: String,
    pub relative: String,
    pub excerpt: String,
}

pub fn resolve_store_root(configured: &str) -> PathBuf {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        AppPaths::notes_dir()
    } else {
        PathBuf::from(trimmed)
    }
}

pub fn repo_notes_root(store_root: &Path, repo_name: &str) -> PathBuf {
    store_root.join(sanitize_entry_name(repo_name, false).unwrap_or_else(|| "notes".into()))
}

pub fn load_tree(repo_notes_root: &Path) -> Vec<NotesTreeItem> {
    if !repo_notes_root.is_dir() {
        return Vec::new();
    }
    load_directory(repo_notes_root)
}

fn load_directory(directory: &Path) -> Vec<NotesTreeItem> {
    let mut folders = Vec::new();
    let mut notes = Vec::new();
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut sorted: Vec<_> = entries.flatten().collect();
    sorted.sort_by_key(|e| e.file_name());
    for entry in sorted {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        match entry.file_type() {
            Ok(t) if t.is_dir() => folders.push(NotesTreeItem {
                children: load_directory(&entry.path()),
                name,
                full_path: entry.path(),
                is_folder: true,
            }),
            Ok(_) if name.to_lowercase().ends_with(".md") => notes.push(NotesTreeItem {
                name,
                full_path: entry.path(),
                is_folder: false,
                children: Vec::new(),
            }),
            _ => {}
        }
    }
    folders.sort_by_key(|a| a.name.to_lowercase());
    notes.sort_by_key(|a| a.name.to_lowercase());
    folders.into_iter().chain(notes).collect()
}

pub fn read_note(path: &Path) -> anyhow::Result<String> {
    Ok(std::fs::read_to_string(path)?)
}

/// BOM-less UTF-8 — a BOM shows up as stray bytes in git diffs.
pub fn write_note(path: &Path, content: &str) -> anyhow::Result<()> {
    crate::jsonio::write_atomically(path, content)
}

pub fn create_note(repo_notes_root: &Path, parent: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let file_name = sanitize_entry_name(name, true)
        .ok_or_else(|| anyhow::anyhow!("The note name is empty."))?;
    ensure_under_root(repo_notes_root, parent)?;
    std::fs::create_dir_all(parent)?;
    let path = parent.join(&file_name);
    std::fs::File::create_new(&path)?;
    Ok(path)
}

/// Saves pasted clipboard image bytes as `<folder>/assets/paste-<local
/// datetime>-<4 hex>.<ext>` and returns the path relative to the note's
/// folder for the markdown ref (forward slashes, so the link works on every
/// platform). `File::create_new` makes the write exclusive: a same-second
/// second paste retries with fresh randomness instead of clobbering.
pub fn insert_image_asset(folder: &Path, ext: &str, bytes: &[u8]) -> std::io::Result<String> {
    if ext.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid image extension",
        ));
    }
    use std::io::Write as _;
    let assets = folder.join("assets");
    std::fs::create_dir_all(&assets)?;
    for _ in 0..8 {
        let rand = paste_rand16();
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let name = format!("paste-{stamp}-{rand:04x}.{ext}");
        match std::fs::File::create_new(assets.join(&name)) {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(format!("assets/{name}"));
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no unique asset name found",
    ))
}

/// Four hex digits of per-call entropy — `RandomState` seeds are random per
/// instance, so no `rand` dependency and same-second pastes still differ.
fn paste_rand16() -> u16 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0),
    );
    hasher.finish() as u16
}

pub fn create_folder(repo_notes_root: &Path, parent: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let folder_name = sanitize_entry_name(name, false)
        .ok_or_else(|| anyhow::anyhow!("The folder name is empty."))?;
    ensure_under_root(repo_notes_root, parent)?;
    let path = parent.join(&folder_name);
    if path.exists() {
        return Err(anyhow::anyhow!("'{folder_name}' already exists."));
    }
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn delete(repo_notes_root: &Path, target: &Path) -> anyhow::Result<()> {
    ensure_under_root(repo_notes_root, target)?;
    if target.is_dir() {
        std::fs::remove_dir_all(target)?;
    } else if target.is_file() {
        std::fs::remove_file(target)?;
    }
    Ok(())
}

pub fn search(repo_notes_root: &Path, term: &str) -> Vec<NotesSearchHit> {
    search_capped(repo_notes_root, term, SEARCH_FILE_CAP)
}

/// The notes-page search scans up to [`SEARCH_FILE_CAP`] files; the global
/// search passes a smaller cap (40 files per repo).
pub fn search_capped(repo_notes_root: &Path, term: &str, file_cap: usize) -> Vec<NotesSearchHit> {
    if !repo_notes_root.is_dir() || term.trim().is_empty() {
        return Vec::new();
    }
    let term = term.trim();
    let mut hits = Vec::new();
    for path in enumerate_note_files(repo_notes_root) {
        if hits.len() >= file_cap {
            break;
        }
        let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let name_match = file_name.to_lowercase().contains(&term.to_lowercase());
        let excerpt = if name_match {
            String::new()
        } else {
            match read_excerpt(&path, term) {
                Some(e) => e,
                None => continue,
            }
        };
        let relative = path
            .strip_prefix(repo_notes_root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        hits.push(NotesSearchHit {
            title: file_name.trim_end_matches(".md").trim_end_matches(".MD").to_string(),
            excerpt: if excerpt.is_empty() { first_content_line(&path).unwrap_or_default() } else { excerpt },
            path,
            relative,
        });
    }
    hits
}

/// Path-name sanitizer: invalid filename chars and separators become '-',
/// edges trimmed; `None` when nothing survives.
pub fn sanitize_entry_name(name: &str, enforce_markdown_extension: bool) -> Option<String> {
    let mut cleaned: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'..='\u{1f}' => '-',
            other => other,
        })
        .collect();
    cleaned = cleaned.trim_matches([' ', '.']).to_string();
    if cleaned.is_empty() {
        return None;
    }
    if enforce_markdown_extension && !cleaned.to_lowercase().ends_with(".md") {
        cleaned.push_str(".md");
    }
    Some(cleaned)
}

/// canonicalize() fails for the not-yet-created parent chains create_note /
/// create_folder validate, and on Windows it returns `\\?\`-prefixed verbatim
/// paths — normalize both sides to plain paths and compare components
/// (case-insensitively on Windows) so mixed forms still pass containment.
fn ensure_under_root(root: &Path, target: &Path) -> anyhow::Result<()> {
    let root = plain_path(root.canonicalize().unwrap_or_else(|_| root.to_path_buf()));
    let target = plain_path(target.canonicalize().unwrap_or_else(|_| target.to_path_buf()));
    let contained = if cfg!(windows) {
        let fold = |p: &Path| -> Vec<String> {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect()
        };
        fold(&target).starts_with(&fold(&root))
    } else {
        target.starts_with(&root)
    };
    if contained {
        Ok(())
    } else {
        Err(anyhow::anyhow!("the path escapes the notes store"))
    }
}

fn plain_path(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    let plain = text
        .strip_prefix(r"\\?\UNC\")
        .map(|rest| format!(r"\\{rest}"))
        .or_else(|| text.strip_prefix(r"\\?\").map(str::to_string));
    match plain {
        Some(plain) => PathBuf::from(plain),
        None => path,
    }
}

fn enumerate_note_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root.to_path_buf());
    while let Some(current) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            match entry.file_type() {
                Ok(t) if t.is_dir() => {
                    if !name.starts_with('.') {
                        queue.push_back(entry.path());
                    }
                }
                Ok(_) if name.to_lowercase().ends_with(".md") => files.push(entry.path()),
                _ => {}
            }
        }
    }
    files
}

fn read_excerpt(path: &Path, term: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > SEARCH_CONTENT_CAP_BYTES {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    for raw_line in content.lines() {
        let line: String = raw_line.split_whitespace().collect::<Vec<_>>().join(" ");
        let Some(index) = line.to_lowercase().find(&term.to_lowercase()) else {
            continue;
        };
        let start = index.saturating_sub(40);
        let end = (start + 160).min(line.len());
        let mut excerpt = String::new();
        if start > 0 {
            excerpt.push('…');
        }
        excerpt.push_str(&line[start..end]);
        if end < line.len() {
            excerpt.push('…');
        }
        return Some(excerpt);
    }
    None
}

fn first_content_line(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    content.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

// ---- markdown editing (the toolbar's pure string ops) --------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct MarkdownEditResult {
    pub text: String,
    pub start: usize,
    pub length: usize,
}

const PLACEHOLDER: &str = "text";

/// Applies one toolbar format to `text` over `start..start+length` (char
/// indices), returning the new text and selection.
pub fn markdown_apply(format: &str, text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let (start, length) = clamp(text, start, length);
    match format {
        "bold" => wrap_selection(text, start, length, "**"),
        "italic" => wrap_selection(text, start, length, "*"),
        "strike" => wrap_selection(text, start, length, "~~"),
        "code" => wrap_selection(text, start, length, "`"),
        "codeblock" => wrap_code_block(text, start, length),
        "link" => insert_link(text, start, length),
        "h1" => toggle_line_prefix(text, start, length, "# "),
        "h2" => toggle_line_prefix(text, start, length, "## "),
        "h3" => toggle_line_prefix(text, start, length, "### "),
        "bullet" => toggle_line_prefix(text, start, length, "- "),
        "task" => toggle_line_prefix(text, start, length, "- [ ] "),
        "numbered" => toggle_numbered(text, start, length),
        "quote" => toggle_line_prefix(text, start, length, "> "),
        "hr" => insert_rule(text, start),
        _ => MarkdownEditResult { text: text.to_string(), start, length },
    }
}

fn clamp(text: &str, start: usize, length: usize) -> (usize, usize) {
    let count = text.chars().count();
    let start = start.min(count);
    let length = length.min(count - start);
    (start, length)
}

fn char_index(text: &str, char_ix: usize) -> usize {
    text.char_indices().nth(char_ix).map(|(i, _)| i).unwrap_or(text.len())
}

fn wrap_selection(text: &str, start: usize, length: usize, marker: &str) -> MarkdownEditResult {
    let marker_chars = marker.chars().count();
    let b_start = char_index(text, start);
    let b_inner_end = char_index(text, start + length);
    // already wrapped exactly once → unwrap
    let before = text[..b_start].ends_with(marker);
    let after = text[b_inner_end..].starts_with(marker);
    if length >= marker_chars * 2 && before && after {
        let unwrapped = format!(
            "{}{}{}",
            &text[..b_start - marker.len()],
            &text[b_start..b_inner_end],
            &text[b_inner_end + marker.len()..]
        );
        return MarkdownEditResult {
            text: unwrapped,
            start: start - marker_chars,
            length,
        };
    }
    let inner = if length > 0 { &text[b_start..b_inner_end] } else { PLACEHOLDER };
    let new_text = format!(
        "{}{marker}{inner}{marker}{}",
        &text[..b_start],
        &text[b_inner_end..]
    );
    MarkdownEditResult {
        text: new_text,
        start: start + marker_chars,
        length: inner.chars().count(),
    }
}

fn wrap_code_block(text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let b_start = char_index(text, start);
    let b_inner_end = char_index(text, start + length);
    if length == 0 {
        let block = format!("```\n{PLACEHOLDER}\n```");
        return MarkdownEditResult {
            text: format!("{}{block}{}", &text[..b_start], &text[b_start..]),
            start: start + 4,
            length: PLACEHOLDER.len(),
        };
    }
    let inner = &text[b_start..b_inner_end];
    MarkdownEditResult {
        text: format!("{}```\n{inner}\n```{}", &text[..b_start], &text[b_inner_end..]),
        start: start + 4,
        length,
    }
}

fn insert_link(text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let b_start = char_index(text, start);
    let b_inner_end = char_index(text, start + length);
    let label = if length > 0 { &text[b_start..b_inner_end] } else { PLACEHOLDER };
    let new_text = format!("{}[{label}](url){}", &text[..b_start], &text[b_inner_end..]);
    MarkdownEditResult {
        text: new_text,
        start: start + label.chars().count() + 3,
        length: 3,
    }
}

fn insert_rule(text: &str, start: usize) -> MarkdownEditResult {
    let b_start = char_index(text, start);
    let before = &text[..b_start];
    let prefix = if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    MarkdownEditResult {
        text: format!("{before}{prefix}---\n\n{}", &text[b_start..]),
        start: start + prefix.chars().count() + 5,
        length: 0,
    }
}

/// Toggles a line prefix over every line touching the selection: when every
/// touched line already carries it (after up to 3 leading spaces) the prefix
/// is removed, otherwise applied.
fn toggle_line_prefix(text: &str, start: usize, length: usize, prefix: &str) -> MarkdownEditResult {
    let chars: Vec<char> = text.chars().collect();
    let selection_end = (start + length).min(chars.len());
    let line_start = chars[..start].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
    let line_end = chars[selection_end..]
        .iter()
        .position(|&c| c == '\n')
        .map(|i| selection_end + i)
        .unwrap_or(chars.len());

    let mut lines: Vec<(usize, String)> = Vec::new(); // (line start, content)
    let mut current = line_start;
    for end in chars[line_start..line_end]
        .iter()
        .enumerate()
        .filter(|&(_, c)| *c == '\n')
        .map(|(i, _)| line_start + i + 1)
        .chain(std::iter::once(line_end))
    {
        lines.push((current, chars[current..end.min(chars.len())].iter().collect()));
        current = end;
    }

    let strip_prefix = |line: &str| -> Option<String> {
        let trimmed = line.trim_start_matches(' ');
        if trimmed.len() == line.len() - trimmed.len() && trimmed.len() > 3 {
            return None;
        }
        trimmed.strip_prefix(prefix).map(|rest| format!("{}{}", &line[..line.len() - trimmed.len()], rest))
    };
    let all_carry = lines.iter().all(|(_, line)| {
        line.trim().is_empty() || line.trim_start_matches(' ').starts_with(prefix.trim_end())
    });

    let mut out = String::new();
    out.extend(chars[..line_start].iter());
    let mut new_start = None;
    let mut consumed = 0usize;
    for (line_ix, (line_begin, line)) in lines.iter().enumerate() {
        let _ = line_begin;
        let replaced = if all_carry && !line.trim().is_empty() {
            strip_prefix(line).unwrap_or_else(|| line.clone())
        } else if line.trim().is_empty() {
            line.clone()
        } else {
            // remove any competing list/heading prefix first
            let stripped = strip_competing(line);
            format!("{prefix}{stripped}")
        };
        if line_ix == 0 {
            new_start = Some(line_start + consumed + prefix.chars().count().min(replaced.chars().count()));
        }
        consumed += replaced.chars().count();
        out.push_str(&replaced);
    }
    let new_len = consumed.saturating_sub(new_start.unwrap_or(line_start));
    out.extend(chars[line_end..].iter());
    MarkdownEditResult {
        text: out,
        start: new_start.unwrap_or(line_start),
        length: new_len,
    }
}

fn strip_competing(line: &str) -> String {
    let trimmed = line.trim_start_matches(' ');
    let indent = &line[..line.len() - trimmed.len()];
    let rest = trimmed
        .trim_start_matches('#')
        .trim_start_matches(' ')
        .trim_start_matches("- [ ] ")
        .trim_start_matches("- [x] ")
        .trim_start_matches("- ")
        .trim_start_matches("* ")
        .trim_start_matches("> ");
    // also strip an existing "N. " marker so renumbering never stacks
    let rest = match rest.split_once(". ") {
        Some((head, tail)) if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) => tail,
        _ => rest,
    };
    format!("{indent}{rest}")
}

/// The numbered-list toggle: strips "N. " markers when every affected line
/// already carries one; otherwise renumbers the selection 1., 2., 3., …
fn toggle_numbered(text: &str, start: usize, length: usize) -> MarkdownEditResult {
    let chars: Vec<char> = text.chars().collect();
    let selection_end = (start + length).min(chars.len());
    let line_start = chars[..start].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
    let line_end = chars[selection_end..]
        .iter()
        .position(|&c| c == '\n')
        .map(|i| selection_end + i)
        .unwrap_or(chars.len());

    let numbered_prefix = |line: &str| -> Option<(String, String)> {
        let trimmed = line.trim_start_matches(' ');
        let (head, tail) = trimmed.split_once(". ")?;
        head.chars()
            .all(|c| c.is_ascii_digit())
            .then(|| (head.to_string(), tail.to_string()))
    };

    let mut affected: Vec<String> = Vec::new();
    let mut current = line_start;
    while current <= line_end {
        let end = chars[current..]
            .iter()
            .position(|&c| c == '\n')
            .map(|i| current + i)
            .unwrap_or(line_end)
            .min(line_end);
        affected.push(chars[current..end].iter().collect());
        if end >= line_end {
            break;
        }
        current = end + 1;
    }

    let numbers: Vec<Option<u32>> = affected
        .iter()
        .map(|line| {
            numbered_prefix(line)
                .and_then(|(head, _)| head.parse::<u32>().ok())
        })
        .collect();
    let all_numbered = affected
        .iter()
        .zip(&numbers)
        .all(|(line, n)| line.trim().is_empty() || n.is_some());
    let numbered: Vec<u32> = numbers.iter().filter_map(|n| *n).collect();
    // already a clean 1..n list → the click toggles it off; stale or duplicate
    // markers → renumber instead
    let sequential = numbered == (1..=numbered.len() as u32).collect::<Vec<u32>>();

    let mut out = String::new();
    out.extend(chars[..line_start].iter());
    let mut new_start = None;
    let mut consumed = 0usize;
    let mut number = 0usize;
    for (ix, line) in affected.iter().enumerate() {
        let replaced = if line.trim().is_empty() {
            line.clone()
        } else if all_numbered && sequential {
            match numbered_prefix(line) {
                Some((_, tail)) => {
                    let indent_len = line.len() - line.trim_start_matches(' ').len();
                    format!("{}{}", &line[..indent_len], tail)
                }
                None => line.clone(),
            }
        } else {
            number += 1;
            format!("{number}. {}", strip_competing(line))
        };
        if ix == 0 {
            new_start = Some(line_start + consumed + replaced.chars().count());
        }
        consumed += replaced.chars().count();
        out.push_str(&replaced);
        if ix + 1 < affected.len() {
            out.push('\n');
            consumed += 1;
        }
    }
    let new_len = consumed.saturating_sub(new_start.unwrap_or(line_start).saturating_sub(line_start));
    out.extend(chars[line_end..].iter());
    MarkdownEditResult {
        text: out,
        start: new_start.unwrap_or(line_start),
        length: new_len,
    }
}

/// Enter inside a list item continues the list (empty item ends it).
pub fn continue_list(text: &str, caret: usize) -> Option<(String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let caret = caret.min(chars.len());
    let line_start = chars[..caret].iter().rposition(|&c| c == '\n').map(|i| i + 1).unwrap_or(0);
    let line: String = chars[line_start..caret].iter().collect();
    let trimmed = line.trim_start_matches(' ');
    let indent = &line[..line.len() - trimmed.len()];

    let markers: &[(&str, bool)] = &[("- ", false), ("* ", false), ("+ ", false), ("> ", false), ("- [ ] ", true), ("- [x] ", true)];
    for (marker, _is_task) in markers {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            if rest.trim().is_empty() {
                // empty item: remove it (and the newline before it) and end
                // the list
                let cut = if line_start > 0 { line_start - 1 } else { line_start };
                let without = format!("{}{}", &text[..char_index(text, cut)], &text[char_index(text, caret)..]);
                let new_caret = cut;
                return Some((without, new_caret));
            }
            let insertion = format!("\n{indent}{marker}");
            let new_caret = caret + insertion.chars().count();
            let at = char_index(text, caret);
            return Some((format!("{}{insertion}{}", &text[..at], &text[at..]), new_caret));
        }
    }
    // ordered list: "1. " with any number
    if let Some(numbered) = trimmed
        .split_once(". ")
        .or_else(|| trimmed.split_once(") "))
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    {
        let n: u64 = numbered.0.parse().ok()?;
        let rest = numbered.1;
        if rest.trim().is_empty() {
            let cut = if line_start > 0 { line_start - 1 } else { line_start };
            let without = format!("{}{}", &text[..char_index(text, cut)], &text[char_index(text, caret)..]);
            return Some((without, cut));
        }
        let insertion = format!("\n{indent}{}. ", n + 1);
        let new_caret = caret + insertion.chars().count();
        let at = char_index(text, caret);
        return Some((format!("{}{insertion}{}", &text[..at], &text[at..]), new_caret));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_invalid_and_enforces_md() {
        assert_eq!(sanitize_entry_name("a/b\\c:d", false).as_deref(), Some("a-b-c-d"));
        assert_eq!(sanitize_entry_name("  ..note.. ", false).as_deref(), Some("note"));
        assert_eq!(sanitize_entry_name("   ", false), None);
        assert_eq!(sanitize_entry_name("todo", true).as_deref(), Some("todo.md"));
        assert_eq!(sanitize_entry_name("TODO.MD", true).as_deref(), Some("TODO.MD"));
    }

    #[test]
    fn wrap_and_unwrap_bold() {
        let result = markdown_apply("bold", "hello world", 6, 5);
        assert_eq!(result.text, "hello **world**");
        assert_eq!((result.start, result.length), (8, 5));

        let unwrapped = markdown_apply("bold", &result.text, result.start, result.length);
        assert_eq!(unwrapped.text, "hello world");
        assert_eq!((unwrapped.start, unwrapped.length), (6, 5));
    }

    #[test]
    fn wrap_with_no_selection_inserts_placeholder() {
        let result = markdown_apply("code", "ab", 1, 0);
        assert_eq!(result.text, "a`text`b");
        assert_eq!((result.start, result.length), (2, 4));
    }

    #[test]
    fn heading_toggles_on_then_off() {
        let on = markdown_apply("h2", "line one\nline two", 0, 15);
        assert_eq!(on.text, "## line one\n## line two");
        let off = markdown_apply("h2", &on.text, on.start, on.length);
        assert_eq!(off.text, "line one\nline two");
    }

    #[test]
    fn bullet_replaces_heading_prefix() {
        let result = markdown_apply("bullet", "## title", 0, 8);
        assert_eq!(result.text, "- title");
    }

    #[test]
    fn numbered_renumbers_then_toggles_off() {
        let on = markdown_apply("numbered", "alpha\nbeta\ngamma", 0, 16);
        assert_eq!(on.text, "1. alpha\n2. beta\n3. gamma");

        // re-applying over the numbered block strips the markers
        let off = markdown_apply("numbered", &on.text, on.start, on.length);
        assert_eq!(off.text, "alpha\nbeta\ngamma");
    }

    #[test]
    fn numbered_skips_blank_lines_when_renumbering() {
        let on = markdown_apply("numbered", "alpha\n\nbeta", 0, 11);
        assert_eq!(on.text, "1. alpha\n\n2. beta");
    }

    #[test]
    fn numbered_replaces_stale_markers_not_stacks() {
        let on = markdown_apply("numbered", "1. alpha\n1. beta", 0, 15);
        assert_eq!(on.text, "1. alpha\n2. beta");
    }

    #[test]
    fn list_continuation_and_termination() {
        let (text, caret) = continue_list("- first item", 12).unwrap();
        assert_eq!(text, "- first item\n- ");
        assert_eq!(caret, 15);

        let (text, _) = continue_list("- first item\n- ", 15).unwrap();
        assert_eq!(text, "- first item");

        let (text, _) = continue_list("1. one", 6).unwrap();
        assert_eq!(text, "1. one\n2. ");
    }

    #[test]
    fn tree_load_and_search_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("dt-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("My Repo");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("alpha.md"), "# Title\n\nhello needle here\n").unwrap();
        std::fs::write(root.join("sub/beta.md"), "nothing relevant\n").unwrap();
        std::fs::write(root.join(".hidden.md"), "needle").unwrap();

        let tree = load_tree(&root);
        assert_eq!(tree.len(), 2);
        assert!(tree[0].is_folder && tree[0].name == "sub");
        assert_eq!(tree[1].name, "alpha.md");

        // hidden FILES match by name (only dot-DIRECTORIES are skipped)
        let hits = search(&root, "needle");
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.excerpt.contains("needle")));

        let note = create_note(&root, &root, "New/Note").unwrap();
        assert!(note.to_string_lossy().ends_with("New-Note.md"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn plain_path_strips_windows_verbatim_prefixes() {
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\C:\Users\e\notes")),
            PathBuf::from(r"C:\Users\e\notes")
        );
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\UNC\server\share\notes")),
            PathBuf::from(r"\\server\share\notes")
        );
        assert_eq!(plain_path(PathBuf::from("/home/e/notes")), PathBuf::from("/home/e/notes"));
    }

    #[test]
    fn insert_image_asset_writes_distinct_files() {
        const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
        let tmp = std::env::temp_dir().join(format!("dt-notes-asset-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let folder = tmp.join("alpha");
        std::fs::create_dir_all(&folder).unwrap();

        let first = insert_image_asset(&folder, "png", PNG_MAGIC).unwrap();
        // the ref is relative with forward slashes — a markdown link
        assert!(
            first.starts_with("assets/paste-") && first.ends_with(".png"),
            "{first}"
        );
        assert!(!first.contains('\\'), "{first}");
        let name = first
            .trim_start_matches("assets/paste-")
            .trim_end_matches(".png");
        let parts: Vec<&str> = name.split('-').collect();
        assert_eq!(parts.len(), 3, "date-time-rand: {first}");
        assert_eq!(
            (parts[0].len(), parts[1].len(), parts[2].len()),
            (8, 6, 4),
            "{first}"
        );
        assert!(parts[2].chars().all(|c| c.is_ascii_hexdigit()), "{first}");

        let written = std::fs::read(folder.join(&first)).unwrap();
        assert_eq!(written, PNG_MAGIC, "bytes land on disk unchanged");

        let second = insert_image_asset(&folder, "png", b"other").unwrap();
        assert_ne!(first, second, "same-second pastes must not collide");

        assert!(insert_image_asset(&folder, "bad/ext", b"").is_err());
        assert!(insert_image_asset(&folder, "", b"").is_err());
        assert_eq!(std::fs::read_dir(folder.join("assets")).unwrap().count(), 2);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn create_note_accepts_verbatim_root_with_new_parent() {
        // root canonicalized (verbatim on Windows), parent not yet on disk
        let tmp = std::env::temp_dir().join(format!("dt-notes-verbatim-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("Repo");
        std::fs::create_dir_all(&root).unwrap();
        let canonical_root = root.canonicalize().unwrap();
        let parent = root.join("New Folder");
        create_note(&canonical_root, &parent, "note").unwrap();
        assert!(parent.join("note.md").is_file());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
