//! Deep headless probe of the merge feature's core: real scratch repositories,
//! the real `git` CLI for every mutation, and the same read/resolve functions
//! the app's net thread calls.
//!
//! Each fixture is created under `/tmp/merge-deep-*`; nothing here writes to a
//! developer's checkout. The last test copies the owner's stopped-merge
//! fixture (read-only source) and drives it through the current code.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use warp_station_core::git::cli::{self, MergeOutcome};
use warp_station_core::git::conflict::{
    self, ConflictPick, ConflictSide, GitConflictBlock, GitConflictFile,
};
use warp_station_core::git::read;

static SEQ: AtomicU64 = AtomicU64::new(0);

/// One throwaway repository. `Drop` removes it, so a failing assertion does
/// not leave state behind for the next run.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    /// A repo under `/tmp/merge-deep-core-*`, with local identity settings and
    /// `rerere` off so repeated fixtures cannot auto-resolve each other.
    fn new(name: &str) -> Self {
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "merge-deep-core-{name}-{}-{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("fixture dir");
        let scratch = Self { dir };
        scratch.git(&["init", "-q", "-b", "main"]);
        scratch.git(&["config", "user.name", "t"]);
        scratch.git(&["config", "user.email", "t@example.com"]);
        scratch.git(&["config", "commit.gpgsign", "false"]);
        scratch.git(&["config", "rerere.enabled", "false"]);
        scratch
    }

    /// A directory that is already a repository (the owner-fixture copy).
    fn existing(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self) -> &Path {
        &self.dir
    }

    fn output(&self, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .output()
            .expect("git runs")
    }

    fn git(&self, args: &[&str]) -> String {
        let out = self.output(args);
        assert!(
            out.status.success(),
            "git {args:?} failed in {}:\nstdout: {}\nstderr: {}",
            self.dir.display(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        std::fs::write(full, text).expect("fixture file");
    }

    fn write_bytes(&self, path: &str, bytes: &[u8]) {
        std::fs::write(self.dir.join(path), bytes).expect("fixture file");
    }

    fn read(&self, path: &str) -> String {
        std::fs::read_to_string(self.dir.join(path)).expect("fixture file read")
    }

    fn commit_all(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }

    fn head_of(&self, reference: &str) -> String {
        self.git(&["rev-parse", reference])
    }

    fn status_short(&self) -> String {
        self.git(&["status", "--short"])
    }

    fn has_merge_head(&self) -> bool {
        self.dir.join(".git/MERGE_HEAD").exists()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The app's async git calls need a reactor for the child process.
fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime")
        .block_on(future)
}

/// `main` and `feat` both edit `shared.txt`'s only line; the fixture is left
/// clean on `main` for the caller to merge.
fn divergent_repo(name: &str) -> Scratch {
    let repo = Scratch::new(name);
    repo.write("shared.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("shared.txt", "feat\n");
    repo.commit_all("feat edit");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("shared.txt", "main\n");
    repo.commit_all("main edit");
    repo
}

/// Run the real merge and require a stop; returns the snapshot after it.
fn stop_merge(repo: &Scratch, source: &str) -> warp_station_core::model::GitChangeSnapshot {
    let outcome = block_on(cli::merge(repo.path(), source));
    assert_eq!(outcome, MergeOutcome::Stopped, "fixture merge must stop");
    assert!(repo.has_merge_head(), "MERGE_HEAD is in place");
    read::change_snapshot(repo.path()).expect("snapshot after the merge")
}

fn only_conflict(repo: &Scratch, path: &str) -> GitConflictFile {
    conflict::conflict_file(repo.path(), path)
        .unwrap_or_else(|| panic!("{path} must read as a text conflict"))
}

fn assert_ranges_are_sane(text: &str, blocks: &[GitConflictBlock]) {
    let mut previous_end = 0;
    for block in blocks {
        assert!(
            block.range.start < block.range.end,
            "a block range is non-empty: {:?}",
            block.range
        );
        assert!(
            block.range.end <= text.len(),
            "block range {:?} fits {} bytes",
            block.range,
            text.len()
        );
        assert!(
            text.is_char_boundary(block.range.start) && text.is_char_boundary(block.range.end),
            "block range {:?} sits on char boundaries",
            block.range
        );
        assert!(
            block.range.start >= previous_end,
            "block ranges ascend without overlap"
        );
        previous_end = block.range.end;
        let slice = &text[block.range.clone()];
        assert!(
            slice.starts_with("<<<<<<<") && slice.contains(">>>>>>>"),
            "the range covers the marker block: {slice:?}"
        );
    }
}

/// The line projection the whole-file panes rely on: the stage lines at
/// `local_lines` / `incoming_lines` / `base_lines` are exactly the block's
/// section texts.
fn assert_stage_projection(file: &GitConflictFile) {
    let local: Vec<&str> = file.local.split_inclusive('\n').collect();
    let incoming: Vec<&str> = file.incoming.split_inclusive('\n').collect();
    let base: Vec<&str> = file
        .base
        .as_deref()
        .unwrap_or("")
        .split_inclusive('\n')
        .collect();
    for block in &file.blocks {
        assert_eq!(
            local[block.local_lines.clone()].concat(),
            block.local,
            "local_lines {:?}",
            block.local_lines
        );
        assert_eq!(
            incoming[block.incoming_lines.clone()].concat(),
            block.incoming,
            "incoming_lines {:?}",
            block.incoming_lines
        );
        if let Some(base_text) = &block.base {
            assert_eq!(
                base[block.base_lines.clone()].concat(),
                *base_text,
                "base_lines {:?}",
                block.base_lines
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Fast-forward
// ---------------------------------------------------------------------------

#[test]
fn a_fast_forward_merge_moves_the_branch_and_leaves_no_state() {
    let repo = Scratch::new("ff");
    repo.write("a.txt", "one\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("a.txt", "two\n");
    repo.commit_all("feat edit");
    repo.git(&["checkout", "-q", "main"]);
    let before = repo.head();
    let feat = repo.head_of("refs/heads/feat");

    let outcome = block_on(cli::merge(repo.path(), "feat"));
    assert_eq!(outcome, MergeOutcome::Merged);
    assert_eq!(repo.head(), feat, "main advanced to the feature commit");
    assert_ne!(repo.head(), before);
    assert!(
        !repo.has_merge_head(),
        "a fast-forward leaves no MERGE_HEAD"
    );
    assert!(!read::merge_in_progress(repo.path()));
    assert_eq!(repo.read("a.txt"), "two\n");
    let snapshot = read::change_snapshot(repo.path()).expect("snapshot");
    assert!(snapshot.merge.is_none(), "no band after a finished merge");
}

// ---------------------------------------------------------------------------
// 2. Already up to date
// ---------------------------------------------------------------------------

#[test]
fn merging_a_contained_branch_is_up_to_date_and_does_not_move_head() {
    let repo = Scratch::new("up-to-date");
    repo.write("a.txt", "one\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("a.txt", "two\n");
    repo.commit_all("feat edit");
    // `feat` contains `main`: merging it is a no-op.
    let before = repo.head();
    let outcome = block_on(cli::merge(repo.path(), "main"));
    assert_eq!(outcome, MergeOutcome::UpToDate);
    assert_eq!(repo.head(), before, "HEAD stayed on the feature commit");
    assert!(!repo.has_merge_head());
    assert!(!read::merge_in_progress(repo.path()));
}

// ---------------------------------------------------------------------------
// 3. Clean non-fast-forward
// ---------------------------------------------------------------------------

#[test]
fn a_clean_non_fast_forward_lands_a_two_parent_merge_commit() {
    let repo = Scratch::new("clean-nff");
    repo.write("base.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("feat.txt", "feat\n");
    repo.commit_all("feat file");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("main.txt", "main\n");
    repo.commit_all("main file");
    let main_before = repo.head();
    let feat = repo.head_of("refs/heads/feat");

    let outcome = block_on(cli::merge(repo.path(), "feat"));
    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(!repo.has_merge_head());
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "Merge branch 'feat'",
        "git wrote the prepared message"
    );
    let parents = repo.git(&["rev-list", "--parents", "-n", "1", "HEAD"]);
    let parents: Vec<&str> = parents.split(' ').collect();
    assert_eq!(parents.len(), 3, "the merge commit has two parents");
    assert!(parents[1..].contains(&main_before.as_str()));
    assert!(parents[1..].contains(&feat.as_str()));
    assert_eq!(repo.read("feat.txt"), "feat\n");
    assert_eq!(repo.read("main.txt"), "main\n");
}

// ---------------------------------------------------------------------------
// 4. Conflict stop, snapshot, resolve, continue, abort
// ---------------------------------------------------------------------------

#[test]
fn a_conflict_stops_the_merge_with_the_state_the_band_reads() {
    let repo = divergent_repo("stop");
    let snapshot = stop_merge(&repo, "feat");
    let merge = snapshot.merge.as_ref().expect("the band's merge state");
    assert_eq!(merge.source, "feat", "MERGE_MSG names the source branch");
    assert_eq!(merge.conflicted, vec!["shared.txt".to_string()]);
    assert!(
        snapshot
            .status_files
            .iter()
            .any(|file| file.path == "shared.txt" && file.status_code == "U"),
        "the list paints the unmerged row"
    );
    assert!(repo.read("shared.txt").contains("<<<<<<<"));
    assert_eq!(repo.git(&["ls-files", "-u"]).lines().count(), 3);

    let abort = block_on(cli::merge_abort(repo.path()));
    assert!(abort.ok, "{:?}", abort.error);
    assert!(!repo.has_merge_head());
    assert!(!read::merge_in_progress(repo.path()));
    assert_eq!(repo.read("shared.txt"), "main\n", "abort restored the tree");
    assert_eq!(repo.status_short(), "", "abort left a clean tree");
    assert!(
        read::change_snapshot(repo.path())
            .expect("snapshot")
            .merge
            .is_none()
    );
}

/// Resolving to the local side leaves the index identical to HEAD: no status
/// rows at all, but the merge is still stopped and must keep its state.
#[test]
fn a_clean_resolved_merge_still_reports_the_merge_state() {
    let repo = divergent_repo("clean-resolved");
    stop_merge(&repo, "feat");
    let resolved = block_on(conflict::resolve(repo.path(), "shared.txt", Some("main\n")));
    assert!(resolved.ok, "{:?}", resolved.error);
    assert_eq!(repo.status_short(), "", "the index matches HEAD");
    let snapshot = read::change_snapshot(repo.path()).expect("snapshot");
    assert!(
        snapshot.status_files.is_empty(),
        "no rows to paint: {:?}",
        snapshot.status_files
    );
    let merge = snapshot
        .merge
        .as_ref()
        .expect("the band must survive the empty status");
    assert!(merge.conflicted.is_empty());
    assert_eq!(merge.source, "feat");
    let done = block_on(cli::merge_continue(repo.path()));
    assert!(done.ok, "{:?}", done.error);
    assert!(!repo.has_merge_head());
}

#[test]
fn resolving_then_continuing_commits_the_prepared_merge_message() {
    let repo = divergent_repo("continue");
    stop_merge(&repo, "feat");
    let before = repo.head();

    let file = only_conflict(&repo, "shared.txt");
    assert_eq!(file.local, "main\n", "stage 2 is the local side");
    assert_eq!(file.incoming, "feat\n", "stage 3 is the incoming side");
    assert_eq!(
        file.worktree,
        "<<<<<<< HEAD\nmain\n=======\nfeat\n>>>>>>> feat\n"
    );
    assert_eq!(file.blocks.len(), 1);
    // The file-level base is the index's stage 1 whenever git has an ancestor
    // (also with the merge style); only the block-level base is diff3's.
    assert!(file.blocks[0].base.is_none());
    assert_ranges_are_sane(&file.worktree, &file.blocks);
    assert_stage_projection(&file);

    // Accept one block: the core writes the marker-free text and stages it.
    let resolved = conflict::assemble(
        &file.worktree,
        &file.blocks,
        &[Some(ConflictSide::Incoming)],
    );
    assert_eq!(resolved, "feat\n");
    let outcome = block_on(conflict::resolve(
        repo.path(),
        "shared.txt",
        Some(&resolved),
    ));
    assert!(outcome.ok, "{:?}", outcome.error);
    assert_eq!(repo.read("shared.txt"), "feat\n");
    assert_eq!(
        repo.git(&["ls-files", "-u"]),
        "",
        "git add cleared the entry"
    );

    let snapshot = read::change_snapshot(repo.path()).expect("snapshot");
    let merge = snapshot
        .merge
        .as_ref()
        .expect("still a merge until Continue");
    assert!(merge.conflicted.is_empty(), "the ready-to-commit phase");
    assert!(
        snapshot
            .status_files
            .iter()
            .any(|file| file.path == "shared.txt" && file.status_code.starts_with('M')),
        "the resolution is staged as a modification: {:?}",
        snapshot
            .status_files
            .iter()
            .map(|file| (file.path.as_str(), file.status_code.as_str()))
            .collect::<Vec<_>>()
    );

    let done = block_on(cli::merge_continue(repo.path()));
    assert!(done.ok, "{:?}", done.error);
    assert!(!repo.has_merge_head());
    assert!(!read::merge_in_progress(repo.path()));
    assert_ne!(repo.head(), before, "Continue moved HEAD");
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "Merge branch 'feat'"
    );
    let body = repo.git(&["log", "-1", "--format=%B"]);
    assert!(
        !body.contains("# Conflicts:"),
        "the comment section is not part of the message: {body:?}"
    );
    assert_eq!(
        repo.git(&["rev-list", "--parents", "-n", "1", "HEAD"])
            .split(' ')
            .count(),
        3
    );
}

// ---------------------------------------------------------------------------
// 5. Multiple files, multiple blocks, picks and whole-file takes
// ---------------------------------------------------------------------------

#[test]
fn multiple_files_and_blocks_assemble_picks_and_bulk_takes() {
    let repo = Scratch::new("multi");
    repo.write(
        "multi.txt",
        "one\nbase1\ntwo\nthree\nfour\nfive\nbase2\nend\n",
    );
    repo.write("other.txt", "base\n");
    repo.write("common.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write(
        "multi.txt",
        "one\nfeat1\ntwo\nthree\nfour\nfive\nfeat2\nend\n",
    );
    repo.write("other.txt", "feat\n");
    repo.write("added.txt", "feat added\n");
    repo.commit_all("feat edits");
    repo.git(&["checkout", "-q", "main"]);
    repo.write(
        "multi.txt",
        "one\nmain1\ntwo\nthree\nfour\nfive\nmain2\nend\n",
    );
    repo.write("other.txt", "main\n");
    repo.write("added.txt", "main added\n");
    repo.commit_all("main edits");
    let snapshot = stop_merge(&repo, "feat");
    let merge = snapshot.merge.as_ref().expect("merge state");
    assert_eq!(merge.conflicted.len(), 3, "{:?}", merge.conflicted);
    for path in ["multi.txt", "other.txt", "added.txt"] {
        assert!(merge.conflicted.iter().any(|item| item == path), "{path}");
    }

    let multi = only_conflict(&repo, "multi.txt");
    assert_eq!(multi.blocks.len(), 2);
    assert_eq!(multi.blocks[0].local, "main1\n");
    assert_eq!(multi.blocks[0].incoming, "feat1\n");
    assert_eq!(multi.blocks[1].local, "main2\n");
    assert_eq!(multi.blocks[1].incoming, "feat2\n");
    assert_eq!(multi.blocks[0].local_lines, 1..2);
    assert_eq!(multi.blocks[1].local_lines, 6..7);
    assert_ranges_are_sane(&multi.worktree, &multi.blocks);
    assert_stage_projection(&multi);

    // Mixed picks, including both-side orders, tracked through the assembly.
    let picks = [
        Some(ConflictPick::LocalThenIncoming),
        Some(ConflictPick::IncomingThenLocal),
    ];
    let (text, ranges) = conflict::assemble_picks_tracked(&multi.worktree, &multi.blocks, &picks);
    assert_eq!(
        text,
        "one\nmain1\nfeat1\ntwo\nthree\nfour\nfive\nfeat2\nmain2\nend\n"
    );
    assert_eq!(&text[ranges[0].clone()], "main1\nfeat1\n");
    assert_eq!(&text[ranges[1].clone()], "feat2\nmain2\n");

    // Whole-file takes: Local keeps main's file; Incoming keeps feat's.
    let bulk = block_on(conflict::resolve_all(
        repo.path(),
        "multi.txt",
        ConflictSide::Local,
    ));
    assert!(bulk.ok, "{:?}", bulk.error);
    assert_eq!(
        repo.read("multi.txt"),
        "one\nmain1\ntwo\nthree\nfour\nfive\nmain2\nend\n"
    );
    let bulk = block_on(conflict::resolve_all(
        repo.path(),
        "other.txt",
        ConflictSide::Incoming,
    ));
    assert!(bulk.ok, "{:?}", bulk.error);
    assert_eq!(repo.read("other.txt"), "feat\n");

    // An add/add conflict: both sides exist and the base stage does not.
    let added = only_conflict(&repo, "added.txt");
    assert_eq!(added.blocks.len(), 1);
    assert!(added.base.is_none(), "add/add has no ancestor stage");
    let text = conflict::assemble(&added.worktree, &added.blocks, &[Some(ConflictSide::Local)]);
    let resolve = block_on(conflict::resolve(repo.path(), "added.txt", Some(&text)));
    assert!(resolve.ok, "{:?}", resolve.error);
    assert_eq!(repo.read("added.txt"), "main added\n");

    let snapshot = read::change_snapshot(repo.path()).expect("snapshot");
    assert!(
        snapshot
            .merge
            .as_ref()
            .is_some_and(|merge| merge.conflicted.is_empty())
    );
    let done = block_on(cli::merge_continue(repo.path()));
    assert!(done.ok, "{:?}", done.error);
    assert!(!repo.has_merge_head());
    let files = repo.git(&["ls-tree", "-r", "--name-only", "HEAD"]);
    for path in ["multi.txt", "other.txt", "added.txt"] {
        assert!(files.contains(path), "the merge commit carries {path}");
    }
}

// ---------------------------------------------------------------------------
// 6. diff3, CRLF, unicode, no final newline, .gitignore
// ---------------------------------------------------------------------------

#[test]
fn a_diff3_conflict_carries_the_ancestor_section() {
    let repo = divergent_repo("diff3");
    repo.git(&["config", "merge.conflictStyle", "diff3"]);
    let snapshot = stop_merge(&repo, "feat");
    assert_eq!(snapshot.merge.expect("merge").source, "feat");
    let file = only_conflict(&repo, "shared.txt");
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.base.as_deref(), Some("base\n"));
    assert_eq!(file.blocks[0].base.as_deref(), Some("base\n"));
    assert_eq!(file.blocks[0].base_lines, 0..1);
    assert_ranges_are_sane(&file.worktree, &file.blocks);
    assert_stage_projection(&file);
    assert!(
        file.worktree.contains("|||||||"),
        "git wrote the base marker: {:?}",
        file.worktree
    );
    let text = conflict::assemble(&file.worktree, &file.blocks, &[Some(ConflictSide::Local)]);
    assert_eq!(text, "main\n");
}

#[test]
fn a_zdiff3_conflict_still_maps_its_sections_to_stage_lines() {
    // zdiff3 hoists the common prefix/suffix outside the markers; the line
    // projection must keep counting those hoisted lines.
    let repo = Scratch::new("zdiff3");
    repo.write("f.c", "int main() {\n    return 0;\n}\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("f.c", "int main() {\n    return 2;\n}\n");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("f.c", "int main() {\n    return 1;\n}\n");
    repo.commit_all("main");
    repo.git(&["config", "merge.conflictStyle", "zdiff3"]);
    stop_merge(&repo, "feat");
    let file = only_conflict(&repo, "f.c");
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.blocks[0].local, "    return 1;\n");
    assert_eq!(file.blocks[0].local_lines, 1..2);
    assert_eq!(file.blocks[0].base.as_deref(), Some("    return 0;\n"));
    assert_eq!(file.blocks[0].base_lines, 1..2);
    assert_ranges_are_sane(&file.worktree, &file.blocks);
}

#[test]
fn crlf_and_unicode_conflicts_keep_their_bytes() {
    let repo = Scratch::new("crlf-unicode");
    repo.write("crlf.txt", "a\r\nbase\r\n");
    repo.write("unicode.txt", "şapka\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("crlf.txt", "a\r\nfeat\r\n");
    repo.write("unicode.txt", "şapka ııı 😀\n");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("crlf.txt", "a\r\nmain\r\n");
    repo.write("unicode.txt", "şapka ğğğ 😀\n");
    repo.commit_all("main");
    stop_merge(&repo, "feat");

    let crlf = only_conflict(&repo, "crlf.txt");
    assert_eq!(crlf.blocks.len(), 1);
    assert_eq!(crlf.local, "a\r\nmain\r\n");
    assert_eq!(crlf.incoming, "a\r\nfeat\r\n");
    assert_eq!(crlf.blocks[0].local, "main\r\n");
    assert_eq!(crlf.blocks[0].incoming, "feat\r\n");
    assert_ranges_are_sane(&crlf.worktree, &crlf.blocks);
    let text = conflict::assemble(
        &crlf.worktree,
        &crlf.blocks,
        &[Some(ConflictSide::Incoming)],
    );
    assert_eq!(text, "a\r\nfeat\r\n");

    let unicode = only_conflict(&repo, "unicode.txt");
    assert_eq!(unicode.blocks.len(), 1);
    assert!(unicode.blocks[0].local.contains("ğğğ"));
    assert!(
        unicode
            .worktree
            .is_char_boundary(unicode.blocks[0].range.start)
            && unicode
                .worktree
                .is_char_boundary(unicode.blocks[0].range.end)
    );
    let text = conflict::assemble(
        &unicode.worktree,
        &unicode.blocks,
        &[Some(ConflictSide::Local)],
    );
    assert_eq!(text, "şapka ğğğ 😀\n");
}

#[test]
fn a_conflict_without_a_final_newline_parses_and_resolves() {
    let repo = Scratch::new("no-final-newline");
    repo.write("f.txt", "base");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("f.txt", "feat");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("f.txt", "main");
    repo.commit_all("main");
    stop_merge(&repo, "feat");
    let file = only_conflict(&repo, "f.txt");
    assert_eq!(file.local, "main", "the stage blob has no final newline");
    assert_eq!(file.incoming, "feat");
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(
        file.blocks[0].local, "main\n",
        "git's marker section adds one"
    );
    assert_ranges_are_sane(&file.worktree, &file.blocks);
    let text = conflict::assemble(&file.worktree, &file.blocks, &[Some(ConflictSide::Local)]);
    assert_eq!(text, "main\n");
}

#[test]
fn a_gitignore_conflict_resolves_like_any_other_text_file() {
    let repo = Scratch::new("gitignore");
    // Mirror the owner's machine: rerere records the preimage on the stop.
    repo.git(&["config", "rerere.enabled", "true"]);
    repo.write(".gitignore", "# Rust\n/target\n*.log\n*.swp\n");
    repo.write("src.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "rust"]);
    repo.write(
        ".gitignore",
        "# Rust\n/target\n*.log\n*.swp\n\n# Editors\n.idea/\n.vs/\n",
    );
    repo.commit_all("rust ignores");
    repo.git(&["checkout", "-q", "main"]);
    repo.write(
        ".gitignore",
        "# Rust\n/target\n*.log\n*.swp\n///////////conflict\n",
    );
    repo.commit_all("local ignore");
    let snapshot = stop_merge(&repo, "rust");
    assert_eq!(snapshot.merge.expect("merge").source, "rust");
    let file = only_conflict(&repo, ".gitignore");
    assert_eq!(file.blocks.len(), 1);
    assert!(file.local.contains("///////////conflict"));
    assert!(file.incoming.contains(".idea/"));
    let text = file.blocks[0].local.clone() + &file.blocks[0].incoming;
    assert!(text.contains(".vscode") || text.contains(".idea/"));
    let resolve = block_on(conflict::resolve(repo.path(), ".gitignore", Some(&text)));
    assert!(resolve.ok, "{:?}", resolve.error);
    assert_eq!(repo.read(".gitignore"), text);
    let done = block_on(cli::merge_continue(repo.path()));
    assert!(done.ok, "{:?}", done.error);
    assert!(!repo.has_merge_head());
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "Merge branch 'rust'"
    );
}

// ---------------------------------------------------------------------------
// 7. Dirty working tree refusal
// ---------------------------------------------------------------------------

#[test]
fn a_dirty_tracked_file_makes_git_refuse_before_any_merge_state() {
    let repo = Scratch::new("dirty-tracked");
    repo.write("shared.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("shared.txt", "feat\n");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("shared.txt", "local work\n");

    let outcome = block_on(cli::merge(repo.path(), "feat"));
    let MergeOutcome::Failed(error) = outcome else {
        panic!("expected Failed, got {outcome:?}");
    };
    let error = error.unwrap_or_default();
    assert!(
        error.contains("would be overwritten"),
        "git's refusal is the message: {error:?}"
    );
    assert!(!repo.has_merge_head(), "no state after a refusal");
    assert!(!read::merge_in_progress(repo.path()));
    assert_eq!(repo.read("shared.txt"), "local work\n", "work kept");
    assert!(
        read::change_snapshot(repo.path())
            .expect("snapshot")
            .merge
            .is_none()
    );
}

#[test]
fn an_untracked_file_in_the_way_is_refused_without_state() {
    let repo = Scratch::new("dirty-untracked");
    repo.write("base.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("new.txt", "from feat\n");
    repo.commit_all("feat adds new.txt");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("new.txt", "local untracked\n");

    let outcome = block_on(cli::merge(repo.path(), "feat"));
    let MergeOutcome::Failed(error) = outcome else {
        panic!("expected Failed, got {outcome:?}");
    };
    let error = error.unwrap_or_default();
    assert!(
        error
            .to_lowercase()
            .contains("untracked working tree files"),
        "{error:?}"
    );
    assert!(!repo.has_merge_head());
    assert_eq!(repo.read("new.txt"), "local untracked\n");
}

// ---------------------------------------------------------------------------
// 8. Delete/modify and binary conflicts
// ---------------------------------------------------------------------------

#[test]
fn a_delete_modify_conflict_has_no_text_file_to_edit() {
    let repo = Scratch::new("delete-modify");
    repo.write("gone.txt", "base\n");
    repo.write("keep.txt", "keep\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.git(&["rm", "-q", "gone.txt"]);
    repo.commit_all("feat deletes");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("gone.txt", "main edited\n");
    repo.commit_all("main edits");
    let snapshot = stop_merge(&repo, "feat");
    assert!(
        snapshot
            .merge
            .expect("merge")
            .conflicted
            .contains(&"gone.txt".to_string())
    );

    // The text model needs both stages; delete/modify has only one.
    assert!(
        conflict::conflict_file(repo.path(), "gone.txt").is_none(),
        "delete/modify has no two-sided text"
    );
    let bulk = block_on(conflict::resolve_all(
        repo.path(),
        "gone.txt",
        ConflictSide::Local,
    ));
    assert!(!bulk.ok, "the bulk take cannot resolve it");
    assert!(
        bulk.error
            .as_deref()
            .is_some_and(|error| error.contains("no text conflict")),
        "{:?}",
        bulk.error
    );

    // The intended delete side is reachable only through `resolve(None)`.
    let done = block_on(conflict::resolve(repo.path(), "gone.txt", None));
    assert!(done.ok, "{:?}", done.error);
    assert!(!repo.path().join("gone.txt").exists());
    assert_eq!(repo.git(&["ls-files", "-u"]), "");
    let snapshot = read::change_snapshot(repo.path()).expect("snapshot");
    assert!(
        snapshot
            .merge
            .as_ref()
            .is_some_and(|merge| merge.conflicted.is_empty())
    );
}

#[test]
fn a_binary_conflict_has_no_text_sides_and_cannot_be_bulk_resolved() {
    let repo = Scratch::new("binary");
    repo.write_bytes("bin.dat", b"\x00base\x01");
    repo.write("text.txt", "base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write_bytes("bin.dat", b"\x00feat\x02");
    repo.commit_all("feat binary");
    repo.git(&["checkout", "-q", "main"]);
    repo.write_bytes("bin.dat", b"\x00main\x03");
    repo.commit_all("main binary");
    let outcome = block_on(cli::merge(repo.path(), "feat"));
    assert_eq!(outcome, MergeOutcome::Stopped);
    let snapshot = read::change_snapshot(repo.path()).expect("snapshot");
    assert!(
        snapshot
            .merge
            .expect("merge")
            .conflicted
            .contains(&"bin.dat".to_string())
    );
    assert!(
        conflict::conflict_file(repo.path(), "bin.dat").is_none(),
        "a binary stage is not text"
    );
    let bulk = block_on(conflict::resolve_all(
        repo.path(),
        "bin.dat",
        ConflictSide::Incoming,
    ));
    assert!(!bulk.ok);
    assert!(
        bulk.error
            .as_deref()
            .is_some_and(|error| error.contains("no text conflict")),
        "{:?}",
        bulk.error
    );
    // Removing the binary is the only core path that clears it.
    let done = block_on(conflict::resolve(repo.path(), "bin.dat", None));
    assert!(done.ok, "{:?}", done.error);
    assert_eq!(repo.git(&["ls-files", "-u"]), "");
}

// ---------------------------------------------------------------------------
// 9. A second merge while one is stopped; Continue/Abort gates
// ---------------------------------------------------------------------------

#[test]
fn a_second_merge_attempt_is_refused_while_the_first_is_stopped() {
    let repo = divergent_repo("while-stopped");
    stop_merge(&repo, "feat");
    let merge_head = repo.read(".git/MERGE_HEAD").trim().to_string();

    let second = block_on(cli::merge(repo.path(), "feat"));
    // `merge` classifies any failed `git merge` while MERGE_HEAD exists as a
    // fresh stop, so a second press reads `Stopped` rather than `Failed`.
    // The branch menu disables Merge while a merge is stopped, so the UI
    // cannot reach this; the state, at least, is unchanged.
    assert_eq!(second, MergeOutcome::Stopped);
    assert!(repo.has_merge_head());
    assert_eq!(repo.read(".git/MERGE_HEAD").trim(), merge_head);

    // Continue with an unmerged path refuses too and changes no state.
    let continued = block_on(cli::merge_continue(repo.path()));
    assert!(!continued.ok, "Continue cannot run with unresolved paths");
    assert!(repo.has_merge_head());
    assert_eq!(
        repo.read("shared.txt"),
        "<<<<<<< HEAD\nmain\n=======\nfeat\n>>>>>>> feat\n"
    );

    let aborted = block_on(cli::merge_abort(repo.path()));
    assert!(aborted.ok, "{:?}", aborted.error);
    assert!(!repo.has_merge_head());
}

/// A small fixture for the shape the owner's fixture hits at scale: git hoists
/// a line before the marker, but stage 2 (local) keeps the section at a
/// different position. The per-stage search must find the section itself, not
/// count the worktree prefix.
#[test]
fn a_hoisted_common_prefix_maps_the_local_section_exactly() {
    let repo = Scratch::new("hoisted-prefix");
    repo.write("f.txt", "A\nB\nC\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("f.txt", "B\nC\nZ\n");
    repo.commit_all("feat");
    repo.git(&["checkout", "-q", "main"]);
    repo.write("f.txt", "A\nB\nCONFLICT_LONG\n");
    repo.commit_all("main");
    stop_merge(&repo, "feat");

    let file = only_conflict(&repo, "f.txt");
    assert_eq!(
        file.worktree, "B\n<<<<<<< HEAD\nCONFLICT_LONG\n=======\nC\nZ\n>>>>>>> feat\n",
        "git hoisted the incoming B line before the marker"
    );
    assert_eq!(file.blocks.len(), 1);
    let block = &file.blocks[0];
    assert_eq!(block.local, "CONFLICT_LONG\n");
    assert_eq!(
        block.local_lines,
        2..3,
        "the section is located in its stage, not counted from the worktree"
    );
    let local: Vec<&str> = file.local.split_inclusive('\n').collect();
    assert_eq!(local[block.local_lines.clone()].concat(), block.local);
    let incoming: Vec<&str> = file.incoming.split_inclusive('\n').collect();
    assert_eq!(
        incoming[block.incoming_lines.clone()].concat(),
        block.incoming
    );
}

// ---------------------------------------------------------------------------
// 10. The owner's stopped merge fixture (copied read-only, then worked on)
// ---------------------------------------------------------------------------

/// The fixture is home-relative: a checkout carrying a stopped merge with the
/// hoisted-prefix shape. A machine without it skips the probe.
fn owner_fixture() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let source = std::path::PathBuf::from(home).join("Developments/KwmSwitcher");
    source.join(".git/MERGE_HEAD").is_file().then_some(source)
}

/// Copies the owner's checkout without the 23G `target/` directory: the merge
/// state lives in `.git` and the worktree, and `/tmp` has no room for build
/// artifacts. The source is only read.
fn copy_owner_fixture(name: &str) -> Option<Scratch> {
    let Some(source) = owner_fixture() else {
        eprintln!("skipping {name}: no stopped merge fixture under $HOME/Developments");
        return None;
    };
    let source = source.as_path();
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let dest = std::env::temp_dir().join(format!(
        "merge-deep-core-kwm-{name}-{}-{seq}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::create_dir_all(&dest).expect("copy dir");
    let status = Command::new("tar")
        .arg("-C")
        .arg(source)
        .arg("--exclude=./target")
        .arg("-cf")
        .arg("-")
        .arg(".")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("tar reads the fixture")
        .wait_with_output()
        .expect("tar finishes");
    assert!(status.status.success(), "tar copy failed");
    let unpack = Command::new("tar")
        .arg("-C")
        .arg(&dest)
        .arg("-xf")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .expect("tar unpacks");
    {
        use std::io::Write;
        let mut stdin = unpack.stdin.as_ref().expect("stdin");
        stdin.write_all(&status.stdout).expect("tar payload");
    }
    let unpack = unpack.wait_with_output().expect("untar finishes");
    assert!(unpack.status.success(), "untar failed");
    assert!(
        dest.join(".git/MERGE_HEAD").is_file(),
        "the copy kept the stopped merge"
    );
    Some(Scratch::existing(dest))
}

#[test]
fn the_owners_stopped_gitignore_merge_reads_resolves_and_commits() {
    let Some(repo) = copy_owner_fixture("gitignore") else {
        return;
    };
    // The snapshot the Changes view builds when the band first appears.
    let snapshot = read::change_snapshot(repo.path()).expect("owner snapshot");
    let merge = snapshot.merge.as_ref().expect("the owner's merge state");
    assert_eq!(merge.source, "rust", "MERGE_MSG says rust");
    assert_eq!(merge.conflicted, vec![".gitignore".to_string()]);
    assert!(
        snapshot.status_files.len() >= 70,
        "the restructure's staged adds/deletes are in the list: {}",
        snapshot.status_files.len()
    );
    assert!(
        snapshot
            .status_files
            .iter()
            .any(|file| file.path == ".gitignore" && file.status_code == "U")
    );
    assert!(
        snapshot
            .status_files
            .iter()
            .filter(|file| file.status_code.starts_with('A'))
            .count()
            >= 30
    );

    // The conflict itself: stages, markers, block ranges.
    let file =
        conflict::conflict_file(repo.path(), ".gitignore").expect(".gitignore is a text conflict");
    assert_eq!(file.path, ".gitignore");
    assert!(
        file.local.contains("///////////conflict"),
        "{:?}",
        file.local
    );
    assert!(file.incoming.contains(".idea/"), "{:?}", file.incoming);
    assert_eq!(file.blocks.len(), 1);
    assert_ranges_are_sane(&file.worktree, &file.blocks);
    assert_eq!(
        &file.worktree[file.blocks[0].range.clone()],
        "<<<<<<< HEAD\n///////////conflict\n=======\n\n# Editors\n.idea/\n.vs/\n.directory\n\n# OS\n.DS_Store\nThumbs.db\n>>>>>>> rust\n"
    );

    // Resolve it the way the editor's Take both would, then Continue.
    let text = file.blocks[0].local.clone() + &file.blocks[0].incoming;
    let resolved = block_on(conflict::resolve(repo.path(), ".gitignore", Some(&text)));
    assert!(resolved.ok, "{:?}", resolved.error);
    assert_eq!(repo.read(".gitignore"), text);
    let snapshot = read::change_snapshot(repo.path()).expect("snapshot after resolve");
    let merge = snapshot.merge.as_ref().expect("ready to continue");
    assert!(merge.conflicted.is_empty());
    let continued = block_on(cli::merge_continue(repo.path()));
    assert!(continued.ok, "{:?}", continued.error);
    assert!(!repo.has_merge_head());
    assert!(!read::merge_in_progress(repo.path()));
    let parents = repo.git(&["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert_eq!(parents.split(' ').count(), 3, "the merge commit landed");
    assert_eq!(
        repo.git(&["log", "-1", "--format=%s"]),
        "Merge branch 'rust'"
    );
    assert!(
        read::change_snapshot(repo.path())
            .expect("snapshot")
            .merge
            .is_none()
    );
}
