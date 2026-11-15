//! The git CLI mutation layer: checkout/branch/clone, fetch/pull/push/publish,
//! stage/unstage/discard and commit — spawned through the shared process
//! runner with the prompt-proof environment and the per-class timeouts, so
//! hooks, gpg signing and credential helpers keep applying (in-process reads
//! never take this path). Args pass as a list; std applies the
//! CommandLineToArgvW-compatible quoting, so no manual quoting helper exists.

use crate::git::read;
use crate::model::GitSyncResult;
use crate::process::{self, ProcessRunOptions};
use std::path::Path;
use std::time::Duration;

/// Upper bound for a single local git invocation.
pub const PROCESS_TIMEOUT: Duration = Duration::from_secs(10);
/// Upper bound for fetch/pull/push — network transfers grow with the payload.
pub const SYNC_TIMEOUT: Duration = Duration::from_secs(60);

pub struct GitOutcome {
    pub ok: bool,
    /// git's actionable stderr line on failure (fatal:/error:/CONFLICT/rejected).
    pub error: Option<String>,
}

impl GitOutcome {
    fn from(result: &process::ProcessRunResult) -> Self {
        if result.timed_out {
            // the timeout path discards stderr, so the generic summarizer
            // would report "failed" with no reason at all
            Self {
                ok: false,
                error: Some("timed out".into()),
            }
        } else if result.succeeded() || result.truncated {
            Self {
                ok: true,
                error: None,
            }
        } else {
            Self {
                ok: false,
                error: summarize_sync_error(&result.stderr),
            }
        }
    }

    pub fn into_sync_result(self) -> GitSyncResult {
        if self.ok {
            GitSyncResult::ok()
        } else {
            GitSyncResult::err(self.error)
        }
    }
}

/// One git invocation: never prompts (GIT_TERMINAL_PROMPT=0), returns stdout on
/// success or the summarized stderr on failure.
async fn run_git(
    working_dir: &Path,
    args: &[String],
    timeout: Duration,
    max_output_bytes: Option<usize>,
) -> anyhow::Result<process::ProcessRunResult> {
    run_git_with_env(working_dir, args, timeout, max_output_bytes, &[]).await
}

/// [`run_git`] with extra environment entries, for the one command that would
/// otherwise open an editor (`git merge --continue`).
async fn run_git_with_env(
    working_dir: &Path,
    args: &[String],
    timeout: Duration,
    max_output_bytes: Option<usize>,
    extra_env: &[(&str, &str)],
) -> anyhow::Result<process::ProcessRunResult> {
    let mut environment: Vec<(String, String)> = vec![("GIT_TERMINAL_PROMPT".into(), "0".into())];
    for (key, value) in extra_env {
        environment.push(((*key).into(), (*value).into()));
    }
    let result = process::run(ProcessRunOptions {
        file_name: "git",
        args,
        working_dir: Some(working_dir),
        timeout: Some(timeout),
        max_output_bytes,
        environment,
        ..Default::default()
    })
    .await?;
    if !result.succeeded() && !result.truncated {
        let stderr = result.stderr.trim();
        let head: String = stderr.chars().take(500).collect();
        tracing::error!(
            "git {} exited {:?} in {}: {head}",
            args.join(" "),
            result.exit_code,
            working_dir.display()
        );
    }
    Ok(result)
}

/// Picks the actionable line from git's stderr — the first
/// fatal/error/conflict/rejection line, falling back to the last non-empty one
/// (transfer chatter like "From origin" would otherwise fill the message).
pub fn summarize_sync_error(stderr: &str) -> Option<String> {
    let lines: Vec<&str> = stderr.split('\n').collect();
    let mut fallback = None;
    for (index, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("fatal: ")
            || line.starts_with("error: ")
            || line.starts_with("CONFLICT")
            || line.starts_with("! [")
        {
            // git lists the paths it would overwrite on the indented lines
            // after the header; the blocked-merge toast names them, so keep
            // those lines (and stop at the sentence after the list)
            if line.contains("would be overwritten") {
                let mut message = line.to_string();
                for next in &lines[index + 1..] {
                    let trimmed = next.trim();
                    if trimmed.is_empty() || !(next.starts_with(' ') || next.starts_with('\t')) {
                        break;
                    }
                    message.push('\n');
                    message.push_str(next);
                }
                return Some(message);
            }
            return Some(line.to_string());
        }
        fallback = Some(line.to_string());
    }
    fallback
}

/// Check out a branch, tag or commit. A remote-tracking pick (`origin/main`)
/// checks out its local tracking equivalent instead of detaching HEAD:
/// the local branch when it exists, otherwise a new one tracking the remote.
/// Anything else — a commit hash, a tag, a local branch — is a plain
/// `git checkout`, so the history drill-down's commit checkout still detaches.
pub async fn checkout(folder: &Path, target: &str) -> GitOutcome {
    // the ref lookup is a git2 read; keep it off the async worker
    let remote_pick = {
        let folder = folder.to_path_buf();
        let target = target.to_string();
        tokio::task::spawn_blocking(move || read::remote_tracking_local(&folder, &target))
            .await
            .ok()
            .flatten()
    };
    if let Some((local, exists)) = remote_pick {
        return if exists {
            checkout_ref(folder, &local).await
        } else {
            run_checkout(
                folder,
                &[
                    "checkout".into(),
                    "-b".into(),
                    local,
                    "--track".into(),
                    target.into(),
                ],
            )
            .await
        };
    }
    checkout_ref(folder, target).await
}

async fn checkout_ref(folder: &Path, target: &str) -> GitOutcome {
    run_checkout(folder, &["checkout".into(), target.into()]).await
}

async fn run_checkout(folder: &Path, args: &[String]) -> GitOutcome {
    match run_git(folder, args, PROCESS_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

pub async fn create_branch(
    folder: &Path,
    branch: &str,
    start_point: Option<&str>,
    checkout: bool,
) -> GitSyncResult {
    let mut args = if checkout {
        vec!["checkout".into(), "-b".into(), branch.into()]
    } else {
        vec!["branch".into(), branch.into()]
    };
    if let Some(start) = start_point.map(str::trim).filter(|s| !s.is_empty()) {
        args.push(start.to_string());
    }
    match run_git(folder, &args, PROCESS_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r).into_sync_result(),
        Err(err) => GitSyncResult::err(Some(err.to_string())),
    }
}

/// `git branch -D`: reflog keeps the tip recoverable, so the two-click
/// arm-confirm in the menu is the only guard it needs.
pub async fn delete_branch(folder: &Path, branch: &str) -> GitSyncResult {
    let args = vec!["branch".into(), "-D".into(), branch.into()];
    match run_git(folder, &args, PROCESS_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r).into_sync_result(),
        Err(err) => GitSyncResult::err(Some(err.to_string())),
    }
}

pub async fn clone(url: &str, parent_directory: &Path, repo_name: &str) -> GitSyncResult {
    clone_with_cancel(url, parent_directory, repo_name, None).await
}

/// [`clone`] with the dialog's stop request attached. Deliberately has no
/// wall-clock cap: a large history over a slow link can legitimately run past
/// any fixed bound, and Cancel kills the tree instead.
pub async fn clone_with_cancel(
    url: &str,
    parent_directory: &Path,
    repo_name: &str,
    cancel: Option<process::CancelFlag>,
) -> GitSyncResult {
    // The working directory is the destination's PARENT — the one command that
    // runs outside any existing repo. A cancelled (killed) clone leaves a
    // partial worktree; remove it so a retry finds the slot free.
    let args = vec![
        "clone".into(),
        url.trim().to_string(),
        repo_name.trim().to_string(),
    ];
    let result = process::run(ProcessRunOptions {
        file_name: "git",
        args: &args,
        working_dir: Some(parent_directory),
        timeout: None,
        cancel,
        environment: vec![("GIT_TERMINAL_PROMPT".into(), "0".into())],
        ..Default::default()
    })
    .await;
    match result {
        Ok(r) if r.succeeded() => GitSyncResult::ok(),
        Ok(r) => {
            remove_partial_clone(parent_directory.join(repo_name)).await;
            if r.cancelled {
                return GitSyncResult::cancelled();
            }
            // surface git's fatal line like every other mutation — "clone
            // failed" alone can't tell an auth failure from a bad URL or an
            // scp-style remote (host:repo is not a valid Windows folder name)
            GitOutcome::from(&r).into_sync_result()
        }
        Err(err) => GitSyncResult::err(Some(err.to_string())),
    }
}

/// On Windows, handles on the killed clone's files (AV, indexer) can outlive
/// the taskkill; retry so a clone retry doesn't die on "folder already
/// exists".
async fn remove_partial_clone(path: std::path::PathBuf) {
    if !path.exists() {
        return;
    }
    for _ in 0..3 {
        if std::fs::remove_dir_all(&path).is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

pub async fn fetch(folder: &Path) -> GitOutcome {
    // the default remote only, no tags: negotiating every tag and sweeping
    // every configured remote is the dominant per-repo cost of a fetch,
    // and the ahead/behind badges track the default remote's refs alone.
    // A folder without a resolvable remote keeps the plain fetch (its outcome
    // is "git rejected it", which the caller reports either way).
    sync(
        folder,
        &fetch_args(read::default_remote_name(folder).as_deref()),
    )
    .await
}

/// The Fetch action's command line. Separated from [`fetch`] so a unit test
/// can pin the shape without a git fixture.
pub fn fetch_args(remote: Option<&str>) -> Vec<String> {
    match remote {
        Some(remote) => vec![
            "fetch".into(),
            "--prune".into(),
            "--no-tags".into(),
            remote.to_string(),
        ],
        None => vec!["fetch".into(), "--prune".into()],
    }
}

pub async fn pull(folder: &Path) -> GitOutcome {
    sync(folder, &["pull".into()]).await
}

pub async fn push(folder: &Path) -> GitOutcome {
    sync(folder, &["push".into()]).await
}

/// What a `git merge` press produced. `Stopped` and `UpToDate` are states the
/// pane shows, not failures: the snapshot carries the conflicts, and merging a
/// branch that is already contained is a no-op worth saying out loud.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeOutcome {
    /// A fast-forward or a merge commit landed.
    Merged,
    /// The merge stopped with `MERGE_HEAD` in place; the worktree has markers.
    Stopped,
    /// The branch was already contained: HEAD did not move.
    UpToDate,
    /// git refused before any merge state existed (dirty tree, bad source).
    Failed(Option<String>),
}

/// `git merge <source>` into the current branch.
///
/// The endings are read back from the repository, not parsed out of git's
/// localized output: a stopped merge is `MERGE_HEAD`, and an unmoved HEAD is
/// the "already up to date" case.
pub async fn merge(folder: &Path, source: &str) -> MergeOutcome {
    let before = read::head_oid(folder);
    let run = run_git(folder, &["merge".into(), source.into()], SYNC_TIMEOUT, None).await;
    match run {
        Ok(result) if result.succeeded() => {
            if read::head_oid(folder) == before {
                MergeOutcome::UpToDate
            } else {
                MergeOutcome::Merged
            }
        }
        Ok(result) => {
            if read::merge_in_progress(folder) {
                MergeOutcome::Stopped
            } else {
                MergeOutcome::Failed(summarize_sync_error(&result.stderr))
            }
        }
        Err(err) => MergeOutcome::Failed(Some(err.to_string())),
    }
}

/// Resolves one side of a conflict without a text surface: the side's stage
/// is checked out into the worktree and staged, or, when that side deleted
/// the file, the unmerged entry is removed. This is the binary and
/// delete/modify path the merge editor cannot show as text.
pub async fn resolve_side(
    folder: &Path,
    path: &str,
    side: crate::git::conflict::ConflictSide,
) -> GitOutcome {
    use crate::git::conflict::ConflictSide;
    let Some((has_local, has_incoming)) = crate::git::conflict::conflict_sides(folder, path) else {
        return GitOutcome {
            ok: false,
            error: Some(format!("{path} has no merge conflict")),
        };
    };
    let present = match side {
        ConflictSide::Local => has_local,
        ConflictSide::Incoming => has_incoming,
    };
    if !present {
        return simple(
            folder,
            &["rm".into(), "-f".into(), "--".into(), path.into()],
        )
        .await;
    }
    let flag = match side {
        ConflictSide::Local => "--ours",
        ConflictSide::Incoming => "--theirs",
    };
    let checkout = simple(
        folder,
        &["checkout".into(), flag.into(), "--".into(), path.into()],
    )
    .await;
    if !checkout.ok {
        return checkout;
    }
    simple(folder, &["add".into(), "--".into(), path.into()]).await
}

/// Commits a stopped merge. `--continue` runs the commit on the prepared
/// `MERGE_MSG`, and `GIT_EDITOR=true` accepts that message without a terminal.
pub async fn merge_continue(folder: &Path) -> GitOutcome {
    match run_git_with_env(
        folder,
        &["merge".into(), "--continue".into()],
        PROCESS_TIMEOUT,
        None,
        &[("GIT_EDITOR", "true")],
    )
    .await
    {
        Ok(result) => GitOutcome::from(&result),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

pub async fn merge_abort(folder: &Path) -> GitOutcome {
    simple(folder, &["merge".into(), "--abort".into()]).await
}

pub async fn publish_branch(folder: &Path, branch: &str) -> GitSyncResult {
    let Some(remote) = read::default_remote_name(folder) else {
        return GitSyncResult::err(Some("no remote configured".into()));
    };
    sync(folder, &["push".into(), "-u".into(), remote, branch.into()])
        .await
        .into_sync_result()
}

async fn sync(folder: &Path, args: &[String]) -> GitOutcome {
    match run_git(folder, args, SYNC_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

pub async fn stage(folder: &Path, path: &str) -> GitOutcome {
    let result = run_git(
        folder,
        &["add".into(), "--".into(), path.into()],
        PROCESS_TIMEOUT,
        None,
    )
    .await;
    match result {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

pub async fn stage_all(folder: &Path) -> GitOutcome {
    simple(folder, &["add".into(), "-A".into()]).await
}

pub async fn unstage(folder: &Path, path: &str) -> GitOutcome {
    simple(
        folder,
        &[
            "reset".into(),
            "-q".into(),
            "HEAD".into(),
            "--".into(),
            path.into(),
        ],
    )
    .await
}

pub async fn unstage_all(folder: &Path) -> GitOutcome {
    simple(folder, &["reset".into(), "-q".into(), "HEAD".into()]).await
}

async fn simple(folder: &Path, args: &[String]) -> GitOutcome {
    match run_git(folder, args, PROCESS_TIMEOUT, None).await {
        Ok(r) => GitOutcome::from(&r),
        Err(err) => GitOutcome {
            ok: false,
            error: Some(err.to_string()),
        },
    }
}

/// Shared tail of the section discards: with `reset_first` the paths are
/// unstaged first (a staged discard's files return to HEAD), then every path
/// still in the index is checked out and every path that is not (untracked
/// files, just-unstaged additions) is deleted.
pub async fn discard_paths(folder: &Path, paths: &[String], reset_first: bool) -> bool {
    if paths.is_empty() {
        return false;
    }
    if reset_first {
        let mut args = vec!["reset".into(), "-q".into(), "HEAD".into(), "--".into()];
        args.extend(paths.iter().cloned());
        if !simple(folder, &args).await.ok {
            return false;
        }
    }

    let in_index = read::indexed_paths(folder, paths).unwrap_or_default();
    let (tracked, untracked): (Vec<&String>, Vec<&String>) =
        paths.iter().partition(|p| in_index.contains(*p));

    if !tracked.is_empty() {
        let mut args = vec!["checkout".into(), "--".into()];
        args.extend(tracked.into_iter().cloned());
        if !simple(folder, &args).await.ok {
            return false;
        }
    }
    if !untracked.is_empty() {
        let mut args = vec!["clean".into(), "-f".into(), "--".into()];
        args.extend(untracked.into_iter().cloned());
        if !simple(folder, &args).await.ok {
            return false;
        }
    }
    true
}

/// Single-file discard: a staged row loses its staged state too (ends at
/// HEAD); an unstaged row keeps the index (a partially staged file keeps its
/// edits).
pub async fn discard_file(folder: &Path, path: &str, is_staged: bool) -> bool {
    if is_staged && !unstage(folder, path).await.ok {
        return false;
    }
    let in_index = read::is_tracked(folder, path);
    let args = if in_index {
        vec!["checkout".into(), "--".into(), path.into()]
    } else {
        vec!["clean".into(), "-f".into(), "--".into(), path.into()]
    };
    simple(folder, &args).await.ok
}

/// Commits with `-F` (the message may carry quotes, newlines or anything a
/// shell-style argument would mangle) and returns the short hash from git's
/// "[branch abc1234] subject" output line.
pub async fn commit(folder: &Path, message: &str) -> Option<String> {
    let message_file = std::env::temp_dir().join(format!(
        "warp-station-commit-{}-{}.msg",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    if let Err(err) = std::fs::write(&message_file, message) {
        tracing::debug!("commit message temp file failed: {err}");
        return None;
    }
    let outcome = run_git(
        folder,
        &[
            "commit".into(),
            "-F".into(),
            message_file.to_string_lossy().to_string(),
        ],
        PROCESS_TIMEOUT,
        None,
    )
    .await;
    let _ = std::fs::remove_file(&message_file);
    let result = outcome.ok()?;
    let first_line = result.stdout.lines().next().unwrap_or("");
    // "[branch abc1234] subject" — pick the hash
    let hash = first_line
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .and_then(|(inside, _)| inside.rsplit(' ').next())
        .unwrap_or("");
    Some(hash.chars().take(7).collect())
}

pub async fn revert_commit(folder: &Path, hash: &str) -> GitOutcome {
    simple(folder, &["revert".into(), "--no-edit".into(), hash.into()]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweep's command line fetches the default remote only, without
    /// tags; a folder the remote lookup cannot resolve keeps the plain
    /// fetch — whose `git` error the pass logs.
    #[test]
    fn fetch_args_fetch_the_default_remote_without_tags() {
        assert_eq!(
            fetch_args(Some("origin")),
            vec!["fetch", "--prune", "--no-tags", "origin"]
        );
        assert_eq!(fetch_args(None), vec!["fetch", "--prune"]);
    }

    #[test]
    fn a_would_be_overwritten_error_keeps_the_file_list() {
        let stderr = "error: Your local changes to the following files would be overwritten by merge:\n\t.gitignore\n\tREADME.md\nPlease commit your changes or stash them before you merge.\nAborting\n";
        let summary = summarize_sync_error(stderr).expect("the error line");
        assert!(
            summary.starts_with("error: Your local changes"),
            "{summary}"
        );
        assert!(summary.contains("\t.gitignore"), "{summary}");
        assert!(summary.contains("\tREADME.md"), "{summary}");
        assert!(!summary.contains("Aborting"), "{summary}");

        // an ordinary failure still summarizes to its actionable line
        assert_eq!(
            summarize_sync_error("From origin\nfatal: unable to access\n"),
            Some("fatal: unable to access".to_string())
        );
    }
}

/// The real `git merge` endings, through the CLI the app spawns: the fixture
/// builds two branches that edit the same line, and every assertion is the
/// repository state the surface reads back.
#[cfg(test)]
mod merge_tests {
    use super::*;
    use git2::build::CheckoutBuilder;

    /// A repo whose HEAD is `refs/heads/main` from the first commit on. The
    /// identity lives in the repo config: a machine without one still gets a
    /// merge commit, and the test never touches the process environment.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ws-merge-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let repo = git2::Repository::init(&dir).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "t").unwrap();
        config.set_str("user.email", "t@example.com").unwrap();
        dir
    }

    fn commit(repo: &git2::Repository, path: &str, text: &str, message: &str) -> git2::Oid {
        std::fs::write(repo.workdir().unwrap().join(path), text).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new(path)).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
    }

    fn switch(repo: &git2::Repository, branch: &str) {
        repo.set_head(&format!("refs/heads/{branch}")).unwrap();
        repo.checkout_head(Some(CheckoutBuilder::new().force()))
            .unwrap();
    }

    /// `main` edits a line, `feat` edits the same line: the merge stops.
    fn conflicting(name: &str) -> std::path::PathBuf {
        let dir = scratch(name);
        let repo = git2::Repository::open(&dir).unwrap();
        commit(&repo, "shared.txt", "base\n", "base");
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feat", &head, false).unwrap();
        switch(&repo, "feat");
        commit(&repo, "shared.txt", "feat\n", "feat edit");
        switch(&repo, "main");
        commit(&repo, "shared.txt", "main\n", "main edit");
        dir
    }

    fn read(dir: &std::path::Path, path: &str) -> String {
        std::fs::read_to_string(dir.join(path)).unwrap()
    }

    #[tokio::test]
    async fn a_conflicting_merge_stops_with_markers_and_aborts_back() {
        let dir = conflicting("abort");
        assert_eq!(merge(&dir, "feat").await, MergeOutcome::Stopped);
        assert!(read::merge_in_progress(&dir), "MERGE_HEAD is in place");
        assert!(
            read(&dir, "shared.txt").contains("<<<<<<<"),
            "the worktree carries conflict markers"
        );

        let aborted = merge_abort(&dir).await;
        assert!(aborted.ok, "{:?}", aborted.error);
        assert!(!read::merge_in_progress(&dir));
        assert_eq!(
            read(&dir, "shared.txt"),
            "main\n",
            "abort restores the side"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn resolving_then_continuing_lands_the_merge_commit() {
        let dir = conflicting("continue");
        assert_eq!(merge(&dir, "feat").await, MergeOutcome::Stopped);
        let before = read::head_oid(&dir);
        std::fs::write(dir.join("shared.txt"), "resolved\n").unwrap();
        assert!(stage(&dir, "shared.txt").await.ok);

        let done = merge_continue(&dir).await;
        assert!(done.ok, "{:?}", done.error);
        assert!(!read::merge_in_progress(&dir));
        assert_ne!(read::head_oid(&dir), before, "a merge commit landed");
        assert_eq!(read(&dir, "shared.txt"), "resolved\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_contained_branch_is_up_to_date_and_a_new_one_merges() {
        let dir = scratch("outcomes");
        {
            let repo = git2::Repository::open(&dir).unwrap();
            commit(&repo, "base.txt", "base\n", "base");
            let head = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("feat", &head, false).unwrap();
        }
        assert_eq!(merge(&dir, "feat").await, MergeOutcome::UpToDate);

        {
            let repo = git2::Repository::open(&dir).unwrap();
            switch(&repo, "feat");
            commit(&repo, "feat.txt", "feat\n", "feat work");
            switch(&repo, "main");
        }
        assert_eq!(merge(&dir, "feat").await, MergeOutcome::Merged);
        assert!(!read::merge_in_progress(&dir));
        assert_eq!(read(&dir, "feat.txt"), "feat\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
