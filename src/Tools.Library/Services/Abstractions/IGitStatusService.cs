using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Singleton that computes local git status (branch, modified count, ahead/behind) and
/// the last commit date for every discovered repo and pushes the results onto the
/// <see cref="Repo"/> entities' runtime-only properties, which the repo cards bind
/// directly. All work happens on background threads via the <c>git</c> CLI with
/// redirected output — calling it never blocks the UI. Refreshes are also triggered
/// automatically whenever <see cref="IRepoService"/> reports new scan data.
/// <para>
/// Also backs the main window's bottom bar with per-repo actions: local branch listing
/// and checkout, <c>git fetch</c> (which also stamps <see cref="Repo.GitLastFetchAt"/>),
/// <c>git pull</c>/<c>git push</c> of the checked-out branch, and the per-file change
/// list for the Changes tab.
/// </para>
/// </summary>
public interface IGitStatusService
{
    /// <summary>
    /// Refreshes the git status of every known repo in the background. Re-entrant:
    /// concurrent calls are coalesced — a call arriving while a refresh is running marks
    /// a pending pass that runs once the current one finishes. Never throws; repos whose
    /// check fails are marked loaded with zeroed counts so the UI stops showing the
    /// "checking…" placeholder.
    /// </summary>
    /// <param name="cancellationToken">Cancels the refresh loop.</param>
    Task RefreshAllAsync(CancellationToken cancellationToken = default);

    /// <summary>
    /// Re-probes a single repo and pushes branch/counts/last-commit onto its entity.
    /// Used by the bottom bar after a checkout or fetch so the fresh state lands without
    /// a full pass. Never throws; a failure marks the repo loaded with zeroed counts.
    /// </summary>
    Task RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Lists the repo's branches (<c>git branch -a</c>): local branches first, then the
    /// remote-tracking branches as of the last fetch. Current branch included but not
    /// specially marked — the caller already knows it from <see cref="Repo.GitBranchName"/>.
    /// Returns an empty list on any failure.
    /// </summary>
    Task<IReadOnlyList<GitBranchRef>> GetBranchesAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Checks out a local branch (<c>git checkout &lt;branch&gt;</c>) and refreshes the
    /// repo's status. Returns false on any failure (unresolvable branch, dirty-tree
    /// conflict, timeout); the caller surfaces the failure without changing state.
    /// </summary>
    Task<bool> CheckoutAsync(Repo repo, string branch, CancellationToken cancellationToken = default);

    /// <summary>
    /// Creates a new branch (<c>git branch &lt;name&gt;</c>) or creates and checks it
    /// out (<c>git checkout -b &lt;name&gt;</c> when <paramref name="checkout"/>), from
    /// <paramref name="startPoint"/> when given (a branch or hash; null = current HEAD),
    /// and refreshes the repo's status. Returns the outcome carrying git's actionable
    /// stderr line on failure (duplicate name, invalid name, timeout).
    /// </summary>
    Task<GitSyncResult> CreateBranchAsync(
        Repo repo,
        string branch,
        string? startPoint = null,
        bool checkout = true,
        CancellationToken cancellationToken = default);

    /// <summary>
    /// Fetches the repo's remotes (<c>git fetch --prune</c>) and refreshes its status so
    /// the ahead/behind counts (measured against local upstream refs and therefore stale
    /// until a fetch) become current. On success stamps
    /// <see cref="Repo.GitLastFetchAt"/> with the completion time. Returns the outcome
    /// carrying git's actionable stderr line on failure (no network, missing
    /// credentials, timeout).
    /// </summary>
    Task<GitSyncResult> FetchAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Pulls the checked-out branch from its upstream (<c>git pull</c> — the user's
    /// configured merge/rebase behavior applies) and refreshes the repo's status so the
    /// ahead/behind counts update. Returns the outcome carrying git's actionable stderr
    /// line on failure (diverged branches, conflicts, no network, timeout).
    /// </summary>
    Task<GitSyncResult> PullAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Pushes the checked-out branch to its upstream (<c>git push</c>) and refreshes the
    /// repo's status. Returns the outcome carrying git's actionable stderr line on
    /// failure (no upstream, rejected non-fast-forward, missing credentials, timeout).
    /// </summary>
    Task<GitSyncResult> PushAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// The full detail of one commit (<c>git show --numstat</c>): every changed file
    /// with its added/deleted line counts. Returns an empty file list on any failure.
    /// </summary>
    Task<GitCommitDetails> GetCommitDetailsAsync(Repo repo, string hash, CancellationToken cancellationToken = default);

    /// <summary>
    /// One commit's patch restricted to a single file (<c>git show &lt;hash&gt; -- &lt;path&gt;</c>),
    /// for the History drawer's per-file expansion. Null on any failure.
    /// </summary>
    Task<string?> GetCommitFilePatchAsync(Repo repo, string hash, string path, CancellationToken cancellationToken = default);

    /// <summary>
    /// Reverts one commit (<c>git revert --no-edit &lt;hash&gt;</c> — a new commit with
    /// git's default message undoes the change) and refreshes the repo's status. Returns
    /// false on any failure (conflict, dirty-tree stop, unresolvable commit).
    /// </summary>
    Task<bool> RevertCommitAsync(Repo repo, string hash, CancellationToken cancellationToken = default);

    /// <summary>
    /// Whether one commit is reachable from any fetched remote-tracking branch
    /// (<c>git branch -r --contains &lt;hash&gt;</c>) — i.e. pushed, so the provider's
    /// web page for it exists. Git errors fail open (true) so the web link is only
    /// hidden on a positive "not contained".
    /// </summary>
    Task<bool> IsCommitPushedAsync(Repo repo, string hash, CancellationToken cancellationToken = default);

    /// <summary>
    /// Lists the repo's working-tree changes file by file (modified, renamed, unmerged
    /// and untracked; every untracked file individually) with each entry's porcelain
    /// status code and — where git reports them — the file's added/deleted line counts.
    /// Returns an empty list on any failure.
    /// </summary>
    Task<IReadOnlyList<GitChangedFile>> GetChangedFilesAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// The same working-tree probe split by staging area: one list for the index
    /// (staged) side and one for the worktree (unstaged) side of every change, each
    /// with its own status letter and numstat line counts. A file modified in both
    /// areas appears in both lists. Returns empty lists on any failure.
    /// </summary>
    Task<GitChangeGroups> GetChangeGroupsAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Stages one file (<c>git add</c>) and refreshes the repo's status so the change
    /// counts and the Changes tab agree. Returns false on any failure.
    /// </summary>
    Task<bool> StageAsync(Repo repo, string path, CancellationToken cancellationToken = default);

    /// <summary>
    /// Stages everything (<c>git add -A</c>, deletions and untracked files included)
    /// and refreshes the repo's status. Returns false on any failure.
    /// </summary>
    Task<bool> StageAllAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Unstages one file (<c>git reset HEAD --</c>) — the index entry returns to HEAD
    /// while the working tree keeps the change — and refreshes the repo's status.
    /// Returns false on any failure (including a repo with no commits yet, where HEAD
    /// does not resolve).
    /// </summary>
    Task<bool> UnstageAsync(Repo repo, string path, CancellationToken cancellationToken = default);

    /// <summary>
    /// Unstages everything (<c>git reset HEAD</c>) — index back to HEAD, working tree
    /// untouched — and refreshes the repo's status. Returns false on any failure.
    /// </summary>
    Task<bool> UnstageAllAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Discards one file's change. An unstaged row reverts the working tree to the
    /// index (<c>git checkout --</c>) — a partially staged file keeps its staged
    /// edits; a staged row unstages first (<c>git reset HEAD --</c>) so the file
    /// returns to HEAD entirely. A file the index does not know (untracked, or a
    /// staged addition just unstaged) is deleted (<c>git clean -f</c>). There is no
    /// undo. Refreshes the repo's status; returns false on any failure.
    /// </summary>
    Task<bool> DiscardFileAsync(Repo repo, string path, bool isStaged, CancellationToken cancellationToken = default);

    /// <summary>
    /// Discards every working-tree change: <c>git reset --hard HEAD</c> drops the
    /// staged and unstaged edits on tracked files, then <c>git clean -fd</c> deletes
    /// untracked files and folders — the usual "discard all" semantics, and there is
    /// no undo. Refreshes the repo's status; returns false on any failure.
    /// </summary>
    Task<GitSyncResult> DiscardAllAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Discards the working-tree side of the given unstaged paths: tracked files
    /// revert to the index (<c>git checkout --</c>) — a partially staged file keeps
    /// its staged edits — and untracked files are deleted (<c>git clean -f</c>).
    /// There is no undo. Refreshes the repo's status; returns false on any failure.
    /// </summary>
    Task<bool> DiscardUnstagedAsync(Repo repo, IReadOnlyList<string> paths, CancellationToken cancellationToken = default);

    /// <summary>
    /// Discards the given staged paths entirely: they are unstaged
    /// (<c>git reset HEAD --</c>) and every file returns to HEAD — a path that was a
    /// staged addition (not in HEAD) is deleted. There is no undo. Refreshes the
    /// repo's status; returns false on any failure.
    /// </summary>
    Task<bool> DiscardStagedAsync(Repo repo, IReadOnlyList<string> paths, CancellationToken cancellationToken = default);

    /// <summary>
    /// Commits the staged index (<c>git commit -F</c> with the message written to a
    /// temp file, so any character survives) and refreshes the repo's status. Returns
    /// the short hash on success, empty when it could not be parsed from git's output,
    /// and null on any failure (nothing staged, hooks rejected, missing identity…).
    /// </summary>
    Task<string?> CommitAsync(Repo repo, string message, CancellationToken cancellationToken = default);

    /// <summary>
    /// The staged diff as a patch (<c>git diff --cached</c>) — the input for an
    /// AI-generated commit message. The read stops at a fixed head (the only consumer
    /// truncates far below it), so very large diffs come back truncated rather than
    /// fully materialized. Empty when nothing is staged; null on any failure.
    /// </summary>
    Task<string?> GetStagedPatchAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Lists the repo's most recent commits (<c>git log -10</c>), newest first, with
    /// short hash, subject, author and commit date. Returns an empty list on any failure.
    /// </summary>
    Task<IReadOnlyList<GitCommitInfo>> GetRecentCommitsAsync(Repo repo, CancellationToken cancellationToken = default);
}
