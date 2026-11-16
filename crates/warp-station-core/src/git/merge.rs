//! The stopped-operation read model: which operation is in progress (merge,
//! rebase, cherry-pick or revert), every unmerged path in it, and the three
//! sides of each text conflict.
//!
//! This is the headless half of the merge editor. It only reads: the index
//! stages, the worktree file and the conflict markers git left in it. Nothing
//! here writes the index or the worktree.

use git2::{IndexConflict, Repository, RepositoryState};
use similar::{DiffOp, TextDiff};
use std::path::Path;

/// A file larger than this is not loaded into the merge view; it falls back to
/// whole-file take-ours/take-theirs like a binary.
pub const MAX_MERGE_FILE_BYTES: usize = 2 * 1024 * 1024;

/// The marker run git writes at the default `conflict-marker-size`.
const MIN_MARKER_LEN: usize = 7;

/// Which stopped operation left the unmerged index behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
}

/// The side a whole-file conflict takes. `Ours` is stage 2, `Theirs` stage 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeSide {
    Ours,
    Theirs,
}

impl MergeOperation {
    /// The operation a repository state names, or `None` when nothing is
    /// stopped (`Clean`) or the state is not a merge-shaped one (`Bisect`).
    pub fn of_state(state: RepositoryState) -> Option<Self> {
        match state {
            RepositoryState::Merge => Some(Self::Merge),
            RepositoryState::Rebase
            | RepositoryState::RebaseInteractive
            | RepositoryState::RebaseMerge
            | RepositoryState::ApplyMailbox
            | RepositoryState::ApplyMailboxOrRebase => Some(Self::Rebase),
            RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
                Some(Self::CherryPick)
            }
            RepositoryState::Revert | RepositoryState::RevertSequence => Some(Self::Revert),
            RepositoryState::Clean | RepositoryState::Bisect => None,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Merge => "Merge",
            Self::Rebase => "Rebase",
            Self::CherryPick => "Cherry-pick",
            Self::Revert => "Revert",
        }
    }

    /// The source-pane labels in stage order: ours (stage 2) first, theirs
    /// (stage 3) second. During a rebase "ours" is the branch being replayed
    /// onto, so git's own words would mislead.
    pub fn side_labels(self) -> (&'static str, &'static str) {
        match self {
            Self::Merge => ("Current", "Incoming"),
            Self::Rebase => ("Onto", "Your commit"),
            Self::CherryPick => ("Current", "Picked commit"),
            Self::Revert => ("Current", "Reverted commit"),
        }
    }
}

/// One stopped operation and its unmerged paths, sorted by path.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeSession {
    pub operation: MergeOperation,
    pub files: Vec<MergeFile>,
}

impl MergeSession {
    /// Conflict blocks plus whole-file rows still waiting for a decision.
    pub fn unresolved(&self) -> usize {
        self.files.iter().map(MergeFile::unresolved).sum()
    }

    pub fn text_files(&self) -> impl Iterator<Item = &TextMerge> {
        self.files.iter().filter_map(|file| match file {
            MergeFile::Text(text) => Some(text),
            MergeFile::WholeFile(_) => None,
        })
    }
}

/// One unmerged path: a text conflict the three-pane editor can show, or a
/// whole-file row that only offers taking a side.
#[derive(Clone, Debug, PartialEq)]
pub enum MergeFile {
    Text(TextMerge),
    WholeFile(WholeFileConflict),
}

impl MergeFile {
    pub fn path(&self) -> &str {
        match self {
            Self::Text(file) => &file.path,
            Self::WholeFile(file) => &file.path,
        }
    }

    pub fn unresolved(&self) -> usize {
        match self {
            Self::Text(file) => file.blocks.len(),
            Self::WholeFile(_) => 1,
        }
    }
}

/// A text conflict: the two stage sides, the worktree result with its markers,
/// the parsed blocks, and the source-pane row alignment.
#[derive(Clone, Debug, PartialEq)]
pub struct TextMerge {
    pub path: String,
    /// Stage 2 lines.
    pub ours: Vec<String>,
    /// Stage 3 lines.
    pub theirs: Vec<String>,
    /// The worktree file as it stands, marker lines included.
    pub result: Vec<String>,
    pub blocks: Vec<ConflictBlock>,
    /// The two source panes, one row per aligned line.
    pub rows: Vec<MergeRow>,
}

impl TextMerge {
    /// Marker blocks still carrying both sides; zero means the buffer is
    /// resolved and may be staged.
    pub fn unresolved(&self) -> usize {
        self.blocks.len()
    }
}

/// One conflict region: where its markers sit in the result, and where each
/// side's text was found in the stage lines.
#[derive(Clone, Debug, PartialEq)]
pub struct ConflictBlock {
    /// The marker block in the result, `<<<<<<<` through `>>>>>>>` inclusive.
    pub result: LineSpan,
    /// Where the ours segment sits in [`TextMerge::ours`], when it was found.
    pub ours: Option<LineSpan>,
    /// Where the theirs segment sits in [`TextMerge::theirs`], when it was
    /// found.
    pub theirs: Option<LineSpan>,
    pub ours_text: Vec<String>,
    pub theirs_text: Vec<String>,
    /// The common-ancestor segment of a diff3/zdiff3 conflict; empty in the
    /// default style.
    pub base_text: Vec<String>,
}

/// A half-open run of lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineSpan {
    pub start: u32,
    pub len: u32,
}

impl LineSpan {
    pub fn end(self) -> u32 {
        self.start + self.len
    }

    pub fn contains(self, line: u32) -> bool {
        line >= self.start && line < self.end()
    }
}

/// One row of the two source panes: the ours line, the theirs line, and the
/// conflict block both belong to when the row is inside one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MergeRow {
    pub ours: Option<u32>,
    pub theirs: Option<u32>,
    pub block: Option<u32>,
}

/// A conflict the editor cannot show as three panes.
#[derive(Clone, Debug, PartialEq)]
pub struct WholeFileConflict {
    pub path: String,
    pub reason: WholeFileReason,
    pub ours_present: bool,
    pub theirs_present: bool,
}

/// Why a path fell back to whole-file take-ours/take-theirs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WholeFileReason {
    Binary,
    DeleteModify,
    MissingResult,
    TooLarge,
    Special,
    Unreadable,
}

/// One conflict still in a result buffer: where its markers sit and the text
/// each side contributed.
#[derive(Clone, Debug, PartialEq)]
pub struct BufferConflict {
    pub result: LineSpan,
    pub ours_text: Vec<String>,
    pub theirs_text: Vec<String>,
    pub base_text: Vec<String>,
}

/// The conflict regions a result buffer still carries. The page re-runs this
/// after every edit, so saving and staging follow the buffer, not the file it
/// was seeded from.
pub fn buffer_conflicts(text: &str) -> Vec<BufferConflict> {
    let lines = lines_of(text);
    parse_blocks(&lines)
        .into_iter()
        .map(|block| BufferConflict {
            result: LineSpan {
                start: block.start as u32,
                len: (block.end - block.start + 1) as u32,
            },
            ours_text: lines[block.ours_start()..block.base.unwrap_or(block.sep)].to_vec(),
            base_text: block
                .base
                .map(|base| lines[base + 1..block.sep].to_vec())
                .unwrap_or_default(),
            theirs_text: lines[block.sep + 1..block.end].to_vec(),
        })
        .collect()
}

/// How many conflicts a buffer still holds: complete blocks plus marker
/// fragments a hand edit left behind. Zero is the only clean answer.
pub fn unresolved_count(text: &str) -> usize {
    let lines = lines_of(text);
    let blocks = parse_blocks(&lines);
    let mut covered = vec![false; lines.len()];
    for block in &blocks {
        covered[block.start..=block.end].fill(true);
    }
    let fragments = lines
        .iter()
        .enumerate()
        .filter(|(index, line)| !covered[*index] && marker_fragment(line))
        .count();
    blocks.len() + fragments
}

fn marker_fragment(line: &str) -> bool {
    ['<', '|', '=', '>'].into_iter().any(|ch| {
        let rest = line.trim_start_matches(ch);
        let run = line.len() - rest.len();
        run >= MIN_MARKER_LEN && (rest.is_empty() || rest.starts_with(' '))
    })
}

impl WholeFileReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::Binary => "Binary file",
            Self::DeleteModify => "Deleted on one side",
            Self::MissingResult => "Missing from the worktree",
            Self::TooLarge => "File too large to edit",
            Self::Special => "Symlink or submodule",
            Self::Unreadable => "Unreadable object",
        }
    }
}

/// Reads the stopped operation at `folder`, or `None` when nothing is stopped
/// or the index has no unmerged paths left.
pub fn session(folder: &Path) -> Option<MergeSession> {
    let repo = Repository::discover(folder).ok()?;
    let operation = MergeOperation::of_state(repo.state())?;
    let index = repo.index().ok()?;
    let conflicts = index.conflicts().ok()?;

    let mut files = Vec::new();
    for conflict in conflicts.flatten() {
        let path = conflict_path(&conflict);
        files.push(build_file(&repo, folder, &conflict, &path));
    }
    if files.is_empty() {
        return None;
    }
    files.sort_by(|left, right| left.path().cmp(right.path()));
    Some(MergeSession { operation, files })
}

fn conflict_path(conflict: &IndexConflict) -> String {
    conflict
        .our
        .as_ref()
        .or(conflict.their.as_ref())
        .or(conflict.ancestor.as_ref())
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
        .unwrap_or_default()
}

fn build_file(repo: &Repository, folder: &Path, conflict: &IndexConflict, path: &str) -> MergeFile {
    let (ours, theirs) = (conflict.our.as_ref(), conflict.their.as_ref());
    let (Some(ours), Some(theirs)) = (ours, theirs) else {
        return whole(
            path,
            WholeFileReason::DeleteModify,
            ours.is_some(),
            theirs.is_some(),
        );
    };
    if special_mode(ours.mode) || special_mode(theirs.mode) {
        return whole(path, WholeFileReason::Special, true, true);
    }
    let (Some(ours_bytes), Some(theirs_bytes)) =
        (blob_bytes(repo, ours.id), blob_bytes(repo, theirs.id))
    else {
        return whole(path, WholeFileReason::Unreadable, true, true);
    };
    if ours_bytes.len() > MAX_MERGE_FILE_BYTES || theirs_bytes.len() > MAX_MERGE_FILE_BYTES {
        return whole(path, WholeFileReason::TooLarge, true, true);
    }

    let result_bytes = match std::fs::read(folder.join(path)) {
        Ok(bytes) => bytes,
        Err(_) => return whole(path, WholeFileReason::MissingResult, true, true),
    };
    if result_bytes.len() > MAX_MERGE_FILE_BYTES {
        return whole(path, WholeFileReason::TooLarge, true, true);
    }

    let (Some(ours_text), Some(theirs_text), Some(result_text)) = (
        decode(&ours_bytes),
        decode(&theirs_bytes),
        decode(&result_bytes),
    ) else {
        return whole(path, WholeFileReason::Binary, true, true);
    };

    let ours = lines_of(&ours_text);
    let theirs = lines_of(&theirs_text);
    let result = lines_of(&result_text);
    let blocks = blocks_with_spans(&result, &ours, &theirs);
    let rows = align_rows(&ours, &theirs, &blocks);
    MergeFile::Text(TextMerge {
        path: path.to_string(),
        ours,
        theirs,
        result,
        blocks,
        rows,
    })
}

fn whole(
    path: &str,
    reason: WholeFileReason,
    ours_present: bool,
    theirs_present: bool,
) -> MergeFile {
    MergeFile::WholeFile(WholeFileConflict {
        path: path.to_string(),
        reason,
        ours_present,
        theirs_present,
    })
}

fn blob_bytes(repo: &Repository, id: git2::Oid) -> Option<Vec<u8>> {
    let blob = repo.find_blob(id).ok()?;
    Some(blob.content().to_vec())
}

fn special_mode(mode: u32) -> bool {
    // 0o160000 is a gitlink, 0o120000 a symlink; neither has text to merge.
    mode == 0o160000 || mode == 0o120000
}

/// A NUL in the first bytes is git's own binary test; invalid UTF-8 is the
/// other sign there is no text to show.
fn decode(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(8000)];
    if head.contains(&0) {
        return None;
    }
    String::from_utf8(bytes.to_vec()).ok()
}

fn lines_of(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

/// A parsed marker block, as line indices into the result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RawBlock {
    start: usize,
    base: Option<usize>,
    sep: usize,
    end: usize,
}

impl RawBlock {
    fn ours_start(self) -> usize {
        self.start + 1
    }
}

/// Parses git's conflict markers: `<<<<<<<`, an optional `|||||||` base
/// section, `=======`, and `>>>>>>>`. A block without a closing marker is not
/// a block — half-deleted markers stay in the text and the file stays
/// unresolved.
fn parse_blocks(lines: &[String]) -> Vec<RawBlock> {
    let mut blocks = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let Some(size) = start_marker(&lines[index]) else {
            index += 1;
            continue;
        };
        let mut base = None;
        let mut sep = None;
        let mut cursor = index + 1;
        while cursor < lines.len() {
            if is_marker(&lines[cursor], '|', size) {
                base.get_or_insert(cursor);
            } else if is_marker(&lines[cursor], '=', size) {
                sep = Some(cursor);
                break;
            } else if is_marker(&lines[cursor], '<', size) || is_marker(&lines[cursor], '>', size) {
                break;
            }
            cursor += 1;
        }
        let Some(sep) = sep else {
            index += 1;
            continue;
        };
        let mut end = None;
        let mut cursor = sep + 1;
        while cursor < lines.len() {
            if is_marker(&lines[cursor], '>', size) {
                end = Some(cursor);
                break;
            }
            if is_marker(&lines[cursor], '<', size) {
                break;
            }
            cursor += 1;
        }
        let Some(end) = end else {
            index += 1;
            continue;
        };
        blocks.push(RawBlock {
            start: index,
            base,
            sep,
            end,
        });
        index = end + 1;
    }
    blocks
}

fn start_marker(line: &str) -> Option<usize> {
    let rest = line.trim_start_matches('<');
    let run = line.len() - rest.len();
    (run >= MIN_MARKER_LEN && (rest.is_empty() || rest.starts_with(' '))).then_some(run)
}

fn is_marker(line: &str, ch: char, size: usize) -> bool {
    let rest = line.trim_start_matches(ch);
    let run = line.len() - rest.len();
    run == size && (rest.is_empty() || rest.starts_with(' '))
}

fn blocks_with_spans(result: &[String], ours: &[String], theirs: &[String]) -> Vec<ConflictBlock> {
    let raw = parse_blocks(result);
    let mut blocks = Vec::with_capacity(raw.len());
    let mut ours_cursor = 0;
    let mut theirs_cursor = 0;
    for block in raw {
        let ours_text = result[block.ours_start()..block.base.unwrap_or(block.sep)].to_vec();
        let base_text = block
            .base
            .map(|base| result[base + 1..block.sep].to_vec())
            .unwrap_or_default();
        let theirs_text = result[block.sep + 1..block.end].to_vec();
        let ours_span = locate(ours, &ours_text, &mut ours_cursor);
        let theirs_span = locate(theirs, &theirs_text, &mut theirs_cursor);
        blocks.push(ConflictBlock {
            result: LineSpan {
                start: block.start as u32,
                len: (block.end - block.start + 1) as u32,
            },
            ours: ours_span,
            theirs: theirs_span,
            ours_text,
            theirs_text,
            base_text,
        });
    }
    blocks
}

/// The first place `segment` appears at or after `cursor`, so blocks keep
/// their file order even when a segment repeats.
fn locate(lines: &[String], segment: &[String], cursor: &mut usize) -> Option<LineSpan> {
    if segment.is_empty() {
        return Some(LineSpan {
            start: *cursor as u32,
            len: 0,
        });
    }
    let mut at = *cursor;
    while at + segment.len() <= lines.len() {
        if lines[at..at + segment.len()]
            .iter()
            .zip(segment)
            .all(|(left, right)| left == right)
        {
            *cursor = at + segment.len();
            return Some(LineSpan {
                start: at as u32,
                len: segment.len() as u32,
            });
        }
        at += 1;
    }
    None
}

/// Aligns the two source panes block by block, so a conflict's rows stay
/// inside its block instead of matching equal lines elsewhere in the file.
fn align_rows(ours: &[String], theirs: &[String], blocks: &[ConflictBlock]) -> Vec<MergeRow> {
    let mut rows = Vec::new();
    let mut ours_cursor = 0;
    let mut theirs_cursor = 0;
    for (index, block) in blocks.iter().enumerate() {
        let (Some(ours_span), Some(theirs_span)) = (block.ours, block.theirs) else {
            continue;
        };
        let ours_start = ours_span.start as usize;
        let theirs_start = theirs_span.start as usize;
        if ours_start < ours_cursor || theirs_start < theirs_cursor {
            continue;
        }
        align_segment(
            &ours[ours_cursor..ours_start],
            &theirs[theirs_cursor..theirs_start],
            None,
            ours_cursor as u32,
            theirs_cursor as u32,
            &mut rows,
        );
        let ours_end = ours_span.end() as usize;
        let theirs_end = theirs_span.end() as usize;
        align_segment(
            &ours[ours_start..ours_end],
            &theirs[theirs_start..theirs_end],
            Some(index as u32),
            ours_start as u32,
            theirs_start as u32,
            &mut rows,
        );
        ours_cursor = ours_end;
        theirs_cursor = theirs_end;
    }
    align_segment(
        &ours[ours_cursor..],
        &theirs[theirs_cursor..],
        None,
        ours_cursor as u32,
        theirs_cursor as u32,
        &mut rows,
    );
    rows
}

fn align_segment(
    ours: &[String],
    theirs: &[String],
    block: Option<u32>,
    ours_base: u32,
    theirs_base: u32,
    rows: &mut Vec<MergeRow>,
) {
    if ours.is_empty() && theirs.is_empty() {
        return;
    }
    let ours_text = ours.join("\n");
    let theirs_text = theirs.join("\n");
    let diff = TextDiff::from_lines(ours_text.as_str(), theirs_text.as_str());
    for op in diff.ops() {
        match *op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for offset in 0..len {
                    rows.push(MergeRow {
                        ours: Some(ours_base + (old_index + offset) as u32),
                        theirs: Some(theirs_base + (new_index + offset) as u32),
                        block,
                    });
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for offset in 0..old_len {
                    rows.push(MergeRow {
                        ours: Some(ours_base + (old_index + offset) as u32),
                        theirs: None,
                        block,
                    });
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for offset in 0..new_len {
                    rows.push(MergeRow {
                        ours: None,
                        theirs: Some(theirs_base + (new_index + offset) as u32),
                        block,
                    });
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                for offset in 0..old_len.max(new_len) {
                    rows.push(MergeRow {
                        ours: (offset < old_len).then(|| ours_base + (old_index + offset) as u32),
                        theirs: (offset < new_len)
                            .then(|| theirs_base + (new_index + offset) as u32),
                        block,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    #[test]
    fn a_merge_style_block_splits_both_sides() {
        let lines = file("a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nb\n");
        let blocks = parse_blocks(&lines);
        assert_eq!(blocks.len(), 1);
        let block = blocks[0];
        assert_eq!(block.start, 1);
        assert_eq!(block.sep, 3);
        assert_eq!(block.end, 5);
        assert_eq!(lines[block.ours_start()..block.sep], ["ours"]);
        assert_eq!(lines[block.sep + 1..block.end], ["theirs"]);
        assert!(block.base.is_none());
    }

    #[test]
    fn a_diff3_block_keeps_the_base_section() {
        let lines = file(
            "a\n<<<<<<< HEAD\nours\n||||||| merged common\nbase\n=======\ntheirs\n>>>>>>> feature\nb\n",
        );
        let blocks = parse_blocks(&lines);
        assert_eq!(blocks.len(), 1);
        let block = blocks[0];
        assert_eq!(block.base, Some(3));
        assert_eq!(lines[block.base.unwrap() + 1..block.sep], ["base"]);
    }

    #[test]
    fn a_custom_marker_size_is_accepted() {
        let lines =
            file("a\n<<<<<<<<<<< HEAD\nours\n===========\ntheirs\n>>>>>>>>>>> feature\nb\n");
        let blocks = parse_blocks(&lines);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start, 1);
        assert_eq!(blocks[0].sep, 3);
        assert_eq!(blocks[0].end, 5);
    }

    #[test]
    fn an_unclosed_marker_is_not_a_block() {
        let lines = file("a\n<<<<<<< HEAD\nours\n=======\ntheirs\n");
        assert!(parse_blocks(&lines).is_empty());
    }

    #[test]
    fn two_blocks_are_parsed_in_order() {
        let lines = file(
            "<<<<<<< HEAD\none\n=======\none\n>>>>>>> feature\nb\n<<<<<<< HEAD\ntwo\n=======\ntwo\n>>>>>>> feature\n",
        );
        let blocks = parse_blocks(&lines);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].end < blocks[1].start);
    }

    #[test]
    fn a_segment_is_located_in_file_order() {
        let lines = file("a\nb\na\n");
        let mut cursor = 0;
        assert_eq!(
            locate(&lines, &["a".to_string()], &mut cursor),
            Some(LineSpan { start: 0, len: 1 })
        );
        assert_eq!(
            locate(&lines, &["a".to_string()], &mut cursor),
            Some(LineSpan { start: 2, len: 1 })
        );
        assert_eq!(locate(&lines, &["a".to_string()], &mut cursor), None);
    }

    #[test]
    fn a_buffer_reports_its_blocks_and_orphans() {
        let text = "a\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\nb\n";
        let conflicts = buffer_conflicts(text);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].ours_text, ["ours"]);
        assert_eq!(conflicts[0].theirs_text, ["theirs"]);
        assert_eq!(conflicts[0].result, LineSpan { start: 1, len: 5 });
        assert_eq!(unresolved_count(text), 1);

        let diff3 =
            "a\n<<<<<<< HEAD\nours\n||||||| base\ncommon\n=======\ntheirs\n>>>>>>> feature\nb\n";
        assert_eq!(buffer_conflicts(diff3)[0].base_text, ["common"]);

        assert_eq!(unresolved_count("a\nresolved\nb\n"), 0);
        // a half-deleted block still carries its marker fragments
        assert_eq!(
            unresolved_count("a\n<<<<<<< HEAD\nours\n=======\ntheirs\n"),
            2
        );
    }
}
