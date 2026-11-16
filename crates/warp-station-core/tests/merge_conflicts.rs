//! The merge read model against real stopped operations: merge, rebase,
//! cherry-pick, whole-file conflicts and the marker styles git writes. Skips
//! when git is unavailable.

use std::path::{Path, PathBuf};
use std::process::Command;
use warp_station_core::git::cli;
use warp_station_core::git::merge::{
    self, LineSpan, MergeFile, MergeOperation, MergeSide, WholeFileReason,
};

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs")
}

fn git_ok(dir: &Path, args: &[&str]) {
    let out = git(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn write(dir: &Path, path: &str, contents: &str) {
    std::fs::write(dir.join(path), contents).expect("the fixture file writes");
}

fn write_bytes(dir: &Path, path: &str, bytes: &[u8]) {
    std::fs::write(dir.join(path), bytes).expect("the fixture file writes");
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ws-merge-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("the fixture directory creates");
    git_ok(&dir, &["init", "-q", "-b", "main"]);
    git_ok(&dir, &["config", "user.email", "t@example.com"]);
    git_ok(&dir, &["config", "user.name", "T"]);
    // byte-exact markers regardless of the machine's core.autocrlf
    git_ok(&dir, &["config", "core.autocrlf", "false"]);
    dir
}

/// A repository stopped on one content conflict in `a.txt`, with `main` on
/// ours and `feature` on theirs.
fn merge_conflict(tag: &str, style: Option<&str>, attrs: Option<&str>) -> PathBuf {
    let dir = scratch(tag);
    if let Some(attrs) = attrs {
        write(&dir, ".gitattributes", attrs);
    }
    write(&dir, "a.txt", "base\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "feature"]);
    write(&dir, "a.txt", "feature\n");
    git_ok(&dir, &["commit", "-qam", "feature"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    write(&dir, "a.txt", "main\n");
    git_ok(&dir, &["commit", "-qam", "main"]);
    if let Some(style) = style {
        git_ok(&dir, &["config", "merge.conflictStyle", style]);
    }
    let out = git(&dir, &["merge", "-q", "feature"]);
    assert!(!out.status.success(), "the merge must stop on a conflict");
    dir
}

/// A repository stopped on a binary conflict in `b.bin`.
fn binary_conflict(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    write_bytes(&dir, "b.bin", &[0, 1, 2, 3]);
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "feature"]);
    write_bytes(&dir, "b.bin", &[0, 9, 2, 3]);
    git_ok(&dir, &["commit", "-qam", "feature"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    write_bytes(&dir, "b.bin", &[0, 7, 2, 3]);
    git_ok(&dir, &["commit", "-qam", "main"]);
    let out = git(&dir, &["merge", "-q", "feature"]);
    assert!(!out.status.success(), "the merge must stop on a conflict");
    dir
}

/// A repository stopped on a delete/modify conflict in `a.txt` (theirs
/// deleted it).
fn delete_modify_conflict(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    write(&dir, "a.txt", "base\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "feature"]);
    git_ok(&dir, &["rm", "-q", "a.txt"]);
    git_ok(&dir, &["commit", "-qm", "delete"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    write(&dir, "a.txt", "main\n");
    git_ok(&dir, &["commit", "-qam", "main"]);
    let out = git(&dir, &["merge", "-q", "feature"]);
    assert!(!out.status.success(), "the merge must stop on a conflict");
    dir
}

#[test]
fn a_merge_conflict_reads_both_sides() {
    if !git_available() {
        return;
    }
    let dir = merge_conflict("plain", None, None);
    let session = merge::session(&dir).expect("a stopped merge");
    assert_eq!(session.operation, MergeOperation::Merge);
    assert_eq!(session.operation.title(), "Merge");
    assert_eq!(session.operation.side_labels(), ("Current", "Incoming"));
    assert_eq!(session.unresolved(), 1);
    assert_eq!(session.files.len(), 1);

    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.path, "a.txt");
    assert_eq!(file.blocks.len(), 1);
    let block = &file.blocks[0];
    assert_eq!(block.ours_text.join("\n"), "main");
    assert_eq!(block.theirs_text.join("\n"), "feature");
    assert!(block.base_text.is_empty());
    assert_eq!(block.ours, Some(LineSpan { start: 0, len: 1 }));
    assert_eq!(block.theirs, Some(LineSpan { start: 0, len: 1 }));
    assert_eq!(block.result, LineSpan { start: 0, len: 5 });
    assert_eq!(
        file.result.join("\n"),
        "<<<<<<< HEAD\nmain\n=======\nfeature\n>>>>>>> feature"
    );
    assert_eq!(file.rows.len(), 1);
    assert_eq!(file.rows[0].block, Some(0));
    assert!(file.rows[0].ours.is_some() && file.rows[0].theirs.is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_diff3_merge_keeps_the_base_segment() {
    if !git_available() {
        return;
    }
    let dir = merge_conflict("diff3", Some("diff3"), None);
    let session = merge::session(&dir).expect("a stopped merge");
    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.blocks[0].base_text.join("\n"), "base");
    assert!(
        file.result.iter().any(|line| line.starts_with("|||||||")),
        "the base section is in the result: {:?}",
        file.result
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_zdiff3_merge_parses() {
    if !git_available() {
        return;
    }
    let dir = merge_conflict("zdiff3", Some("zdiff3"), None);
    let session = merge::session(&dir).expect("a stopped merge");
    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(file.blocks[0].base_text.join("\n"), "base");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_custom_marker_size_is_read() {
    if !git_available() {
        return;
    }
    let dir = merge_conflict("marker-size", None, Some("a.txt conflict-marker-size=11\n"));
    let session = merge::session(&dir).expect("a stopped merge");
    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.blocks.len(), 1);
    let run = file.result[0].chars().take_while(|ch| *ch == '<').count();
    assert_eq!(run, 11, "the first line is a eleven-character marker");
    assert_eq!(file.blocks[0].ours_text.join("\n"), "main");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn two_blocks_keep_their_rows_apart() {
    if !git_available() {
        return;
    }
    let dir = scratch("two-blocks");
    write(&dir, "a.txt", "a\nb\nc\nd\ne\nf\ng\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "feature"]);
    write(&dir, "a.txt", "A\nb\nc\nd\ne\nF\ng\n");
    git_ok(&dir, &["commit", "-qam", "feature"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    write(&dir, "a.txt", "X\nb\nc\nd\ne\nY\ng\n");
    git_ok(&dir, &["commit", "-qam", "main"]);
    let out = git(&dir, &["merge", "-q", "feature"]);
    assert!(!out.status.success(), "the merge must stop on a conflict");

    let session = merge::session(&dir).expect("a stopped merge");
    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.blocks.len(), 2);
    assert_eq!(file.blocks[0].ours_text.join("\n"), "X");
    assert_eq!(file.blocks[1].ours_text.join("\n"), "Y");
    assert_eq!(file.blocks[0].ours, Some(LineSpan { start: 0, len: 1 }));
    assert_eq!(file.blocks[1].ours, Some(LineSpan { start: 5, len: 1 }));
    // one row per block, four context rows between them, one after
    assert_eq!(file.rows.len(), 7);
    assert_eq!(file.rows[0].block, Some(0));
    assert!(file.rows[1..5].iter().all(|row| row.block.is_none()));
    assert_eq!(file.rows[5].block, Some(1));
    assert_eq!(file.rows[6].block, None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_delete_modify_conflict_is_whole_file() {
    if !git_available() {
        return;
    }
    let dir = delete_modify_conflict("delete-modify");

    let session = merge::session(&dir).expect("a stopped merge");
    let MergeFile::WholeFile(file) = &session.files[0] else {
        panic!("a delete/modify conflict has no text sides");
    };
    assert_eq!(file.path, "a.txt");
    assert_eq!(file.reason, WholeFileReason::DeleteModify);
    assert!(file.ours_present);
    assert!(!file.theirs_present);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_binary_conflict_is_whole_file() {
    if !git_available() {
        return;
    }
    let dir = binary_conflict("binary");

    let session = merge::session(&dir).expect("a stopped merge");
    let MergeFile::WholeFile(file) = &session.files[0] else {
        panic!("a binary conflict has no text sides");
    };
    assert_eq!(file.reason, WholeFileReason::Binary);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_resolved_merge_leaves_the_rail() {
    if !git_available() {
        return;
    }
    let dir = merge_conflict("rail", None, None);
    let probe = warp_station_core::git::read::probe(&dir).expect("a probe");
    assert_eq!(probe.merge, Some(MergeOperation::Merge));

    write(&dir, "a.txt", "resolved\n");
    git_ok(&dir, &["add", "a.txt"]);
    let probe = warp_station_core::git::read::probe(&dir).expect("a probe");
    assert_eq!(
        probe.merge, None,
        "no unmerged paths remain, so the rail leaves"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn taking_a_side_resolves_a_binary_conflict() {
    if !git_available() {
        return;
    }
    let dir = binary_conflict("binary-take");
    let outcome = cli::take_side(&dir, "b.bin", MergeSide::Theirs).await;
    assert!(outcome.ok, "take theirs failed: {:?}", outcome.error);
    let bytes = std::fs::read(dir.join("b.bin")).expect("the taken file reads");
    assert_eq!(bytes, vec![0, 9, 2, 3], "theirs is [0, 9, 2, 3]");
    assert!(merge::session(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn taking_the_deleted_side_removes_the_path() {
    if !git_available() {
        return;
    }
    let dir = delete_modify_conflict("delete-take");
    let outcome = cli::take_side(&dir, "a.txt", MergeSide::Theirs).await;
    assert!(outcome.ok, "take theirs failed: {:?}", outcome.error);
    assert!(
        !dir.join("a.txt").exists(),
        "theirs deleted the file, so the path is gone"
    );
    assert!(merge::session(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_resolved_file_leaves_the_session() {
    if !git_available() {
        return;
    }
    let dir = merge_conflict("resolved", None, None);
    write(&dir, "a.txt", "resolved\n");
    git_ok(&dir, &["add", "a.txt"]);
    assert!(
        merge::session(&dir).is_none(),
        "no unmerged entries remain, so the page has nothing to show"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_rebase_conflict_is_onto_your_commit() {
    if !git_available() {
        return;
    }
    let dir = scratch("rebase");
    write(&dir, "a.txt", "base\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "feature"]);
    write(&dir, "a.txt", "feature\n");
    git_ok(&dir, &["commit", "-qam", "feature"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    write(&dir, "a.txt", "main\n");
    git_ok(&dir, &["commit", "-qam", "main"]);
    git_ok(&dir, &["checkout", "-q", "feature"]);
    let out = git(&dir, &["rebase", "main"]);
    assert!(!out.status.success(), "the rebase must stop on a conflict");

    let session = merge::session(&dir).expect("a stopped rebase");
    assert_eq!(session.operation, MergeOperation::Rebase);
    assert_eq!(session.operation.side_labels(), ("Onto", "Your commit"));
    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.blocks[0].ours_text.join("\n"), "main");
    assert_eq!(file.blocks[0].theirs_text.join("\n"), "feature");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_cherry_pick_conflict_is_current_picked() {
    if !git_available() {
        return;
    }
    let dir = scratch("cherry-pick");
    write(&dir, "a.txt", "base\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "feature"]);
    write(&dir, "a.txt", "feature\n");
    git_ok(&dir, &["commit", "-qam", "feature"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    write(&dir, "a.txt", "main\n");
    git_ok(&dir, &["commit", "-qam", "main"]);
    let out = git(&dir, &["cherry-pick", "feature"]);
    assert!(!out.status.success(), "the pick must stop on a conflict");

    let session = merge::session(&dir).expect("a stopped cherry-pick");
    assert_eq!(session.operation, MergeOperation::CherryPick);
    assert_eq!(
        session.operation.side_labels(),
        ("Current", "Picked commit")
    );
    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.blocks[0].ours_text.join("\n"), "main");
    assert_eq!(file.blocks[0].theirs_text.join("\n"), "feature");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_clean_repository_has_no_session() {
    if !git_available() {
        return;
    }
    let dir = scratch("clean");
    write(&dir, "a.txt", "one\n");
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    assert!(merge::session(&dir).is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_large_conflict_file_is_read() {
    if !git_available() {
        return;
    }
    let dir = scratch("large");
    let base: String = (0..10_000).map(|line| format!("line {line}\n")).collect();
    write(&dir, "a.txt", &base);
    git_ok(&dir, &["add", "-A"]);
    git_ok(&dir, &["commit", "-qm", "base"]);
    git_ok(&dir, &["checkout", "-q", "-b", "feature"]);
    let theirs = base.replace("line 9999\n", "theirs\n");
    write(&dir, "a.txt", &theirs);
    git_ok(&dir, &["commit", "-qam", "feature"]);
    git_ok(&dir, &["checkout", "-q", "main"]);
    let ours = base.replace("line 9999\n", "ours\n");
    write(&dir, "a.txt", &ours);
    git_ok(&dir, &["commit", "-qam", "main"]);
    let out = git(&dir, &["merge", "-q", "feature"]);
    assert!(!out.status.success(), "the merge must stop on a conflict");

    let started = std::time::Instant::now();
    let session = merge::session(&dir).expect("a stopped merge");
    let elapsed = started.elapsed();
    let MergeFile::Text(file) = &session.files[0] else {
        panic!("a.txt is a text conflict");
    };
    assert_eq!(file.blocks.len(), 1);
    assert_eq!(
        file.blocks[0].ours,
        Some(LineSpan {
            start: 9999,
            len: 1
        })
    );
    assert_eq!(file.rows.len(), 10_000);
    println!("10k-line merge read in {elapsed:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
