//! Merge-conflict reads and resolutions: the three index stages for one path,
//! the worktree file's conflict-marker blocks, and the write-then-stage step
//! the merge editor's Accept runs.
//!
//! The stage texts come from `git2::Index::conflicts()`; the block boundaries
//! come from the worktree file git wrote, so the editor shows exactly the
//! markers the user would see in an editor, without guessing a three-way
//! alignment. A file without markers (binary, delete/modify, an unknown
//! conflict style) comes back with no blocks and the UI falls back to
//! whole-side takes.

use git2::Repository;
use std::ops::Range;
use std::path::Path;

/// One file's conflict: the index stages, the worktree text, and the marker
/// blocks the worktree carries. `base` is present only when the repository
/// uses `merge.conflictStyle=diff3` (or zdiff3).
#[derive(Clone, Debug, Default)]
pub struct GitConflictFile {
    pub path: String,
    /// Stage 2: the current branch's side.
    pub local: String,
    /// Stage 3: the branch being merged in.
    pub incoming: String,
    /// Stage 1: the merge base, when the index has it.
    pub base: Option<String>,
    /// The worktree file with its markers; empty when the file is gone.
    pub worktree: String,
    /// One entry per marker block, ascending.
    pub blocks: Vec<GitConflictBlock>,
}

/// One marker block: where it sits in the worktree text, and the two (or
/// three) sides git left between the markers.
#[derive(Clone, Debug, Default)]
pub struct GitConflictBlock {
    /// The byte range from the `<<<<<<<` line's start through the `>>>>>>>`
    /// line's newline: what a resolution replaces.
    pub range: Range<usize>,
    pub local: String,
    pub incoming: String,
    pub base: Option<String>,
    /// The block's lines in each stage file, for the whole-file panes. The
    /// projection is exact for a content conflict: removing the markers and
    /// the other side leaves the stage file, so the line index before the
    /// block is the stage line the section starts on. A merge-style conflict
    /// has no base section, so `base_lines` is an empty range there.
    pub local_lines: Range<usize>,
    pub incoming_lines: Range<usize>,
    pub base_lines: Range<usize>,
}

/// The three sides a whole-file take can choose when the worktree has no
/// marker blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConflictSide {
    Local,
    Incoming,
}

/// Reads one conflicted path. `None` when the path is not in conflict, the
/// repository cannot be opened, or a side is binary (no text to resolve).
pub fn conflict_file(folder: &Path, path: &str) -> Option<GitConflictFile> {
    let repo = Repository::discover(folder).ok()?;
    let index = repo.index().ok()?;
    let mut stages = None;
    for entry in index.conflicts().ok()? {
        let Ok(entry) = entry else { continue };
        let entry_path = entry
            .our
            .as_ref()
            .or(entry.their.as_ref())
            .or(entry.ancestor.as_ref())
            .and_then(|index_entry| {
                std::str::from_utf8(&index_entry.path)
                    .ok()
                    .map(str::to_string)
            });
        if entry_path.as_deref() == Some(path) {
            stages = Some(entry);
            break;
        }
    }
    let stages = stages?;
    let side_text = |entry: &Option<git2::IndexEntry>| -> Option<String> {
        let entry = entry.as_ref()?;
        let blob = repo.find_blob(entry.id).ok()?;
        if blob.is_binary() {
            return None;
        }
        Some(String::from_utf8_lossy(blob.content()).into_owned())
    };
    let local = side_text(&stages.our)?;
    let incoming = side_text(&stages.their)?;
    let base = side_text(&stages.ancestor);
    let worktree = std::fs::read_to_string(folder.join(path)).unwrap_or_default();
    let blocks = parse_conflict_blocks(&worktree).unwrap_or_default();
    Some(GitConflictFile {
        path: path.to_string(),
        local,
        incoming,
        base,
        worktree,
        blocks,
    })
}

/// Parses git's conflict markers into blocks. `None` for a malformed file (a
/// missing end marker), which the editor treats as "no blocks" rather than
/// inventing boundaries. Handles the `merge` style and the `diff3`/`zdiff3`
/// base section; CRLF lines keep their carriage return in the side texts.
pub fn parse_conflict_blocks(text: &str) -> Option<Vec<GitConflictBlock>> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Normal,
        Local,
        Base,
        Incoming,
    }
    let mut blocks = Vec::new();
    let mut state = State::Normal;
    let mut offset = 0usize;
    let mut start = 0usize;
    let mut local = String::new();
    let mut base = String::new();
    let mut incoming = String::new();
    // Lines accumulated in each stage's projection: the common lines plus the
    // sections of that side. At a block start this is the stage line the
    // section begins on; a content conflict's projection is the whole stage
    // file, so the mapping is exact.
    let mut local_line = 0usize;
    let mut incoming_line = 0usize;
    let mut base_line = 0usize;
    let mut local_start = 0usize;
    let mut incoming_start = 0usize;
    let mut base_start = 0usize;
    let mut local_count = 0usize;
    let mut incoming_count = 0usize;
    let mut base_count = 0usize;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\r', '\n']);
        match state {
            State::Normal => {
                if marker_run(bare, '<') >= 7 {
                    state = State::Local;
                    start = offset;
                    local.clear();
                    base.clear();
                    incoming.clear();
                    local_start = local_line;
                    incoming_start = incoming_line;
                    base_start = base_line;
                    local_count = 0;
                    incoming_count = 0;
                    base_count = 0;
                } else {
                    local_line += 1;
                    incoming_line += 1;
                    base_line += 1;
                }
            }
            State::Local => {
                if marker_run(bare, '|') >= 7 {
                    state = State::Base;
                } else if marker_run(bare, '=') >= 7 {
                    state = State::Incoming;
                } else if marker_run(bare, '>') >= 7 {
                    // a merge-style block without a `=======` line is not
                    // something git writes; refuse rather than guess
                    return None;
                } else {
                    local.push_str(line);
                    local_count += 1;
                }
            }
            State::Base => {
                if marker_run(bare, '=') >= 7 {
                    state = State::Incoming;
                } else if marker_run(bare, '>') >= 7 {
                    return None;
                } else {
                    base.push_str(line);
                    base_count += 1;
                }
            }
            State::Incoming => {
                if marker_run(bare, '>') >= 7 {
                    blocks.push(GitConflictBlock {
                        range: start..offset + line.len(),
                        local: std::mem::take(&mut local),
                        incoming: std::mem::take(&mut incoming),
                        base: (!base.is_empty()).then(|| std::mem::take(&mut base)),
                        local_lines: local_start..local_start + local_count,
                        incoming_lines: incoming_start..incoming_start + incoming_count,
                        base_lines: base_start..base_start + base_count,
                    });
                    local_line = local_start + local_count;
                    incoming_line = incoming_start + incoming_count;
                    base_line = base_start + base_count;
                    state = State::Normal;
                } else if marker_run(bare, '=') >= 7
                    || marker_run(bare, '<') >= 7
                    || marker_run(bare, '|') >= 7
                {
                    return None;
                } else {
                    incoming.push_str(line);
                    incoming_count += 1;
                }
            }
        }
        offset += line.len();
    }
    if state == State::Normal {
        Some(blocks)
    } else {
        None
    }
}

fn marker_run(line: &str, marker: char) -> usize {
    line.chars().take_while(|ch| *ch == marker).count()
}

/// The worktree text with every block replaced by the chosen side, in order.
/// `choices[i]` is block `i`; a missing or `None` choice keeps the block's
/// incoming side out and writes nothing for it (the UI only assembles when
/// every block is chosen).
pub fn assemble(
    text: &str,
    blocks: &[GitConflictBlock],
    choices: &[Option<ConflictSide>],
) -> String {
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    for (block, choice) in blocks.iter().zip(choices) {
        if block.range.start < cursor || block.range.end > text.len() {
            continue;
        }
        result.push_str(&text[cursor..block.range.start]);
        match choice {
            Some(ConflictSide::Local) => result.push_str(&block.local),
            Some(ConflictSide::Incoming) => result.push_str(&block.incoming),
            None => result.push_str(&text[block.range.clone()]),
        }
        cursor = block.range.end;
    }
    result.push_str(&text[cursor..]);
    result
}

/// Resolves every block of one file to the same side, in one step: the
/// conflicted row's context menu's "Keep Local" / "Take Incoming". A file with
/// no marker blocks takes that whole stage; a file that is not a text conflict
/// fails with a readable line.
pub async fn resolve_all(
    folder: &Path,
    path: &str,
    side: ConflictSide,
) -> crate::git::cli::GitOutcome {
    let Some(file) = conflict_file(folder, path) else {
        return crate::git::cli::GitOutcome {
            ok: false,
            error: Some(format!("{path} has no text conflict to resolve")),
        };
    };
    let text = if file.blocks.is_empty() {
        match side {
            ConflictSide::Local => file.local.clone(),
            ConflictSide::Incoming => file.incoming.clone(),
        }
    } else {
        let choices = vec![Some(side); file.blocks.len()];
        assemble(&file.worktree, &file.blocks, &choices)
    };
    resolve(folder, path, Some(&text)).await
}

/// Writes the resolved text and stages it: `git add` is what clears the
/// unmerged entry, so Accept and a manual `git add` are the same signal.
pub async fn resolve(folder: &Path, path: &str, text: Option<&str>) -> crate::git::cli::GitOutcome {
    let full = folder.join(path);
    let written = match text {
        Some(text) => std::fs::write(&full, text),
        None => match std::fs::remove_file(&full) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        },
    };
    if let Err(err) = written {
        return crate::git::cli::GitOutcome {
            ok: false,
            error: Some(format!("could not write {path}: {err}")),
        };
    }
    crate::git::cli::stage(folder, path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_merge_style_blocks() {
        let text = "one\n<<<<<<< HEAD\nlocal\n=======\nincoming\n>>>>>>> feat\ntwo\n";
        let blocks = parse_conflict_blocks(text).expect("well-formed");
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].local, "local\n");
        assert_eq!(blocks[0].incoming, "incoming\n");
        assert_eq!(blocks[0].base, None);
        assert_eq!(
            &text[blocks[0].range.clone()],
            "<<<<<<< HEAD\nlocal\n=======\nincoming\n>>>>>>> feat\n"
        );
    }

    #[test]
    fn parses_diff3_base_sections_and_multiple_blocks() {
        let text = "<<<<<<< HEAD\na\n||||||| base\nb\n=======\nc\n>>>>>>> main\nmid\n<<<<<<< HEAD\nd\n=======\ne\n>>>>>>> main\n";
        let blocks = parse_conflict_blocks(text).expect("well-formed");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].base.as_deref(), Some("b\n"));
        assert_eq!(blocks[0].local_lines, 0..1);
        assert_eq!(blocks[0].base_lines, 0..1);
        assert_eq!(blocks[1].local, "d\n");
        assert_eq!(blocks[1].incoming, "e\n");
        // "mid" is one common line, so the second block starts on line 2
        assert_eq!(blocks[1].local_lines, 2..3);
        assert_eq!(blocks[1].incoming_lines, 2..3);
    }

    #[test]
    fn a_missing_end_marker_is_not_parsed() {
        assert!(parse_conflict_blocks("<<<<<<< HEAD\nlocal\n=======\nincoming\n").is_none());
    }

    #[test]
    fn assembly_replaces_ranges_and_keeps_the_rest() {
        let text = "a\n<<<<<<< HEAD\nl\n=======\ni\n>>>>>>> m\nz\n";
        let blocks = parse_conflict_blocks(text).expect("well-formed");
        let local = assemble(text, &blocks, &[Some(ConflictSide::Local)]);
        assert_eq!(local, "a\nl\nz\n");
        let incoming = assemble(text, &blocks, &[Some(ConflictSide::Incoming)]);
        assert_eq!(incoming, "a\ni\nz\n");
        let unresolved = assemble(text, &blocks, &[None]);
        assert_eq!(unresolved, text);
    }

    #[test]
    fn crlf_lines_keep_their_carriage_returns() {
        let text = "a\r\n<<<<<<< HEAD\r\nl\r\n=======\r\ni\r\n>>>>>>> m\r\nz\r\n";
        let blocks = parse_conflict_blocks(text).expect("well-formed");
        assert_eq!(blocks[0].local, "l\r\n");
        assert_eq!(
            &text[blocks[0].range.clone()],
            "<<<<<<< HEAD\r\nl\r\n=======\r\ni\r\n>>>>>>> m\r\n"
        );
    }

    /// A real stopped merge, read and resolved through the same functions the
    /// app calls: the sides come from the index, the markers from the worktree,
    /// and `resolve` writes + stages so the path leaves the conflict list.
    #[tokio::test]
    async fn reads_and_stages_a_real_stopped_merge() {
        use std::path::PathBuf;
        fn git(dir: &std::path::Path, args: &[&str]) {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .envs([
                    ("GIT_AUTHOR_NAME", "t"),
                    ("GIT_AUTHOR_EMAIL", "t@example.com"),
                    ("GIT_COMMITTER_NAME", "t"),
                    ("GIT_COMMITTER_EMAIL", "t@example.com"),
                ])
                .status()
                .expect("git runs");
            assert!(status.success(), "git {args:?} failed");
        }
        let dir: PathBuf =
            std::env::temp_dir().join(format!("ws-conflict-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        git(&dir, &["init", "-b", "main"]);
        std::fs::write(dir.join("shared.txt"), "base\n").expect("base");
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-m", "base"]);
        git(&dir, &["checkout", "-b", "feat"]);
        std::fs::write(dir.join("shared.txt"), "feat\n").expect("feat");
        git(&dir, &["commit", "-am", "feat edit"]);
        git(&dir, &["checkout", "main"]);
        std::fs::write(dir.join("shared.txt"), "main\n").expect("main");
        git(&dir, &["commit", "-am", "main edit"]);
        let status = std::process::Command::new("git")
            .args(["merge", "feat"])
            .current_dir(&dir)
            .status()
            .expect("git runs");
        assert!(!status.success(), "the fixture merge must stop");

        let file = conflict_file(&dir, "shared.txt").expect("the conflict file");
        assert_eq!(file.local, "main\n", "stage 2 is the local side");
        assert_eq!(file.incoming, "feat\n", "stage 3 is the incoming side");
        assert_eq!(file.blocks.len(), 1);
        let text = assemble(
            &file.worktree,
            &file.blocks,
            &[Some(ConflictSide::Incoming)],
        );
        assert_eq!(text, "feat\n");
        let outcome = resolve(&dir, "shared.txt", Some(&text)).await;
        assert!(outcome.ok, "{:?}", outcome.error);
        let snapshot = crate::git::read::change_snapshot(&dir).expect("snapshot");
        assert_eq!(
            snapshot.merge.map(|merge| merge.conflicted.len()),
            Some(0),
            "the staged path leaves the conflict list"
        );
        assert!(
            !std::fs::read_to_string(dir.join("shared.txt"))
                .expect("resolved file")
                .contains("<<<<<<<")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The bulk path: every block to one side, written and staged in one call.
    #[tokio::test]
    async fn resolve_all_takes_the_whole_file() {
        use std::path::PathBuf;
        fn git(dir: &std::path::Path, args: &[&str]) {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .envs([
                    ("GIT_AUTHOR_NAME", "t"),
                    ("GIT_AUTHOR_EMAIL", "t@example.com"),
                    ("GIT_COMMITTER_NAME", "t"),
                    ("GIT_COMMITTER_EMAIL", "t@example.com"),
                ])
                .status()
                .expect("git runs");
            assert!(status.success(), "git {args:?} failed");
        }
        let dir: PathBuf =
            std::env::temp_dir().join(format!("ws-conflict-bulk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        git(&dir, &["init", "-b", "main"]);
        std::fs::write(dir.join("shared.txt"), "base\n").expect("base");
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-m", "base"]);
        git(&dir, &["checkout", "-b", "feat"]);
        std::fs::write(dir.join("shared.txt"), "feat\n").expect("feat");
        git(&dir, &["commit", "-am", "feat edit"]);
        git(&dir, &["checkout", "main"]);
        std::fs::write(dir.join("shared.txt"), "main\n").expect("main");
        git(&dir, &["commit", "-am", "main edit"]);
        let status = std::process::Command::new("git")
            .args(["merge", "feat"])
            .current_dir(&dir)
            .status()
            .expect("git runs");
        assert!(!status.success(), "the fixture merge must stop");

        let outcome = resolve_all(&dir, "shared.txt", ConflictSide::Local).await;
        assert!(outcome.ok, "{:?}", outcome.error);
        assert_eq!(
            std::fs::read_to_string(dir.join("shared.txt")).expect("resolved"),
            "main\n"
        );
        let snapshot = crate::git::read::change_snapshot(&dir).expect("snapshot");
        assert_eq!(
            snapshot.merge.map(|merge| merge.conflicted.len()),
            Some(0),
            "the whole-file take clears the unmerged entry"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
