using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Shared refresh-coalescing state machine for the background refresh services
/// (<see cref="GitStatusService"/> and the <see cref="RepoActivityServiceBase{TActivity}"/>
/// providers, which compose it): while one refresh pass runs, additional triggers only
/// flag a follow-up pass, so a burst of triggers never stacks concurrent passes.
/// <para>
/// <see cref="RunCoalescedAsync"/> owns the loop: the "is a loop running?" check and the
/// runner hand-off are atomic under one lock, so no trigger is ever lost between the last
/// pass and the loop exiting, and a trigger arriving during a pass re-arms the loop for
/// exactly one more pass. The pass body is supplied by the owning service — guard checks
/// and failure logging stay there; <see cref="RunThrottledPassAsync"/> provides the shared
/// mechanics for one pass: a repo snapshot plus a <see cref="SemaphoreSlim"/>-bounded
/// fan-out over it.
/// </para>
/// </summary>
public sealed class RefreshCoalescer
{
    private readonly IRepoService _repoService;

    /// <summary>Guards <see cref="_isRefreshing"/>/<see cref="_loopTask"/>/<see cref="_refreshPending"/>.</summary>
    private readonly object _sync = new();

    /// <summary>
    /// True while a refresh pass loop is running. Cleared in the same critical section
    /// that decides the loop exits — a task's completion is NOT atomic with that decision,
    /// so testing <see cref="_loopTask"/>.IsCompleted instead would let a trigger arriving
    /// in between join a loop that never runs another pass (its request silently lost).
    /// </summary>
    private bool _isRefreshing;

    /// <summary>The running loop's task — the handle joining callers await.</summary>
    private Task? _loopTask;

    /// <summary>Set when a refresh is requested while one is running; runs another pass after.</summary>
    private bool _refreshPending;

    public RefreshCoalescer(IRepoService repoService)
    {
        _repoService = repoService;
    }

    /// <summary>
    /// Runs <paramref name="passBody"/> in a coalescing loop: calls arriving while a loop
    /// is running just flag exactly one follow-up pass, and the loop drains the flag
    /// before exiting. Such a caller JOINS the running loop — the returned task is the
    /// loop itself, not an already-completed one — so awaiting callers (the Repos page's
    /// Refresh button) outwait the follow-up pass their trigger armed instead of seeing
    /// the refresh "done" while a pass is still settling.
    /// </summary>
    public Task RunCoalescedAsync(Func<CancellationToken, Task> passBody, CancellationToken cancellationToken)
    {
        lock (_sync)
        {
            if (_isRefreshing)
            {
                _refreshPending = true;
                return _loopTask ?? Task.CompletedTask;
            }

            _isRefreshing = true;
            _loopTask = RunLoopAsync(passBody, cancellationToken);
            return _loopTask;
        }
    }

    /// <summary>
    /// The loop behind <see cref="_loopTask"/>. The normal exit clears
    /// <see cref="_isRefreshing"/> under the lock together with the exit decision, so a
    /// trigger arriving afterwards starts a fresh loop rather than joining this one.
    /// </summary>
    private async Task RunLoopAsync(Func<CancellationToken, Task> passBody, CancellationToken cancellationToken)
    {
        var exitedNormally = false;
        try
        {
            while (true)
            {
                lock (_sync)
                {
                    _refreshPending = false;
                }

                await passBody(cancellationToken);

                lock (_sync)
                {
                    if (!_refreshPending || cancellationToken.IsCancellationRequested)
                    {
                        _isRefreshing = false;
                        exitedNormally = true;
                        return;
                    }
                }
            }
        }
        finally
        {
            // An exception escaping the pass body (e.g. a canceled token mid-pass) skips
            // the normal exit, so release the flag here — but only on that path: after a
            // normal exit another loop may already own the flag, and clearing it again
            // would let a third trigger start a concurrent pass.
            if (!exitedNormally)
            {
                lock (_sync)
                {
                    _isRefreshing = false;
                }
            }
        }
    }

    /// <summary>
    /// One throttled pass over every known repo: the list is snapshotted first (a rescan
    /// may swap RepoService.Repos mid-refresh; probing a since-removed repo is harmless —
    /// its entity is simply orphaned), then <paramref name="repoWorker"/> runs for each
    /// repo, bounded by <paramref name="maxParallelism"/>. Exceptions escaping a worker
    /// propagate to the caller — each service wraps this with its own failure logging.
    /// </summary>
    public async Task RunThrottledPassAsync(
        int maxParallelism,
        Func<Repo, CancellationToken, Task> repoWorker,
        CancellationToken cancellationToken)
    {
        var repos = _repoService.Repos
            .Where(r => !string.IsNullOrWhiteSpace(r.FolderPath))
            .ToList();
        if (repos.Count == 0) return;

        using var throttle = new SemaphoreSlim(maxParallelism);
        var tasks = repos.Select(async repo =>
        {
            await throttle.WaitAsync(cancellationToken);
            try
            {
                await repoWorker(repo, cancellationToken);
            }
            finally
            {
                throttle.Release();
            }
        });
        await Task.WhenAll(tasks);
    }
}
