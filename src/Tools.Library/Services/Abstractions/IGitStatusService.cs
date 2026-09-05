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
/// and the per-file change list for the Changes tab.
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
    /// Lists the repo's local branches (<c>git branch</c>), current branch included but
    /// not specially marked — the caller already knows it from <see cref="Repo.GitBranchName"/>.
    /// Returns an empty list on any failure.
    /// </summary>
    Task<IReadOnlyList<string>> GetBranchesAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Checks out a local branch (<c>git checkout &lt;branch&gt;</c>) and refreshes the
    /// repo's status. Returns false on any failure (unresolvable branch, dirty-tree
    /// conflict, timeout); the caller surfaces the failure without changing state.
    /// </summary>
    Task<bool> CheckoutAsync(Repo repo, string branch, CancellationToken cancellationToken = default);

    /// <summary>
    /// Fetches the repo's remotes (<c>git fetch --prune</c>) and refreshes its status so
    /// the ahead/behind counts (measured against local upstream refs and therefore stale
    /// until a fetch) become current. On success stamps
    /// <see cref="Repo.GitLastFetchAt"/> with the completion time. Returns false on any
    /// failure (no network, missing credentials, timeout).
    /// </summary>
    Task<bool> FetchAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Lists the repo's working-tree changes file by file (modified, renamed, unmerged
    /// and untracked; every untracked file individually) with each entry's porcelain
    /// status code and — where git reports them — the file's added/deleted line counts.
    /// Returns an empty list on any failure.
    /// </summary>
    Task<IReadOnlyList<GitChangedFile>> GetChangedFilesAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// Lists the repo's most recent commits (<c>git log -10</c>), newest first, with
    /// short hash, subject, author and commit date. Returns an empty list on any failure.
    /// </summary>
    Task<IReadOnlyList<GitCommitInfo>> GetRecentCommitsAsync(Repo repo, CancellationToken cancellationToken = default);
}
