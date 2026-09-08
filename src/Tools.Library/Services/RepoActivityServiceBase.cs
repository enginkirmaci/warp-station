using System.Collections.Concurrent;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Template-method base for the hosting-provider activity services
/// (<see cref="GitHubService"/>, <see cref="AzureDevOpsService"/>): it owns everything
/// the two used to duplicate — the settings' column flag, the coalescing
/// <see cref="RefreshAllAsync"/> loop, the throttled pass over every repo, the
/// auto-refresh on <see cref="IRepoService"/>.Changed, the per-folder activity cache
/// and the disabled-service guard (<see cref="FetchGuardedAsync{TActivity}"/>) — while
/// the derived services supply only the provider specifics: readiness, configuration,
/// the typed per-repo fetch, and how a repo settles to unavailable.
/// </summary>
public abstract class RepoActivityServiceBase<TActivity> : IRepoActivityService
{
    /// <summary>The live repo list — derived providers prune their per-folder caches against it.</summary>
    protected readonly IRepoService _repoService;

    /// <summary>Owns the coalescing refresh loop and the throttled pass over every repo.</summary>
    private readonly RefreshCoalescer _coalescer;

    /// <summary>Volatile snapshot of the last Configure's column flag (the VM reconfigures per navigation).</summary>
    private volatile bool _enabled;

    /// <summary>Last fetched activity per repo folder, backing the details dialog's instant open.</summary>
    private readonly ConcurrentDictionary<string, TActivity> _activityByFolder = new(StringComparer.Ordinal);

    protected RepoActivityServiceBase(IRepoService repoService)
    {
        _repoService = repoService;
        _coalescer = new RefreshCoalescer(repoService);
        _repoService.Changed += OnRepoServiceChanged;
    }

    /// <inheritdoc/>
    public bool IsEnabled => _enabled && IsProviderReady;

    /// <summary>
    /// Provider-specific readiness beyond the settings flag: a resolvable
    /// <c>gh</c> (<see cref="GitHubService"/>), a usable personal access token
    /// (<see cref="AzureDevOpsService"/>).
    /// </summary>
    protected abstract bool IsProviderReady { get; }

    /// <summary>The settings flag that turns this provider's column on.</summary>
    protected abstract bool ResolveColumnFlag(ReposSettings settings);

    /// <summary>
    /// Applies the provider-specific settings (executable paths, tokens, rejected-
    /// credential resets, warnings), after the base has stored the column flag — read
    /// it there as <see cref="ColumnEnabled"/>.
    /// </summary>
    protected abstract void ConfigureProvider(ReposSettings settings);

    /// <inheritdoc/>
    public void Configure(ReposSettings settings)
    {
        _enabled = ResolveColumnFlag(settings);
        ConfigureProvider(settings);
    }

    /// <summary>Whether the column flag itself is on, as opposed to
    /// <see cref="IsProviderReady"/> failing — provider configuration warns against it.</summary>
    protected bool ColumnEnabled => _enabled;

    /// <summary>How many repos are probed concurrently; keeps API traffic polite.</summary>
    protected virtual int MaxParallelism => 3;

    /// <summary>Log source name for pass failures ("GitHub", "Azure DevOps").</summary>
    protected abstract string ServiceName { get; }

    /// <summary>The typed per-repo fetch, as a plain task for the pass's worker.</summary>
    protected abstract Task FetchRepoAsync(Repo repo, CancellationToken cancellationToken);

    /// <inheritdoc/>
    public async Task RefreshAllAsync(CancellationToken cancellationToken = default)
    {
        if (!IsEnabled) return;
        await _coalescer.RunCoalescedAsync(RefreshPassAsync, cancellationToken);
    }

    /// <summary>
    /// One throttled pass over every known repo — the snapshot and the
    /// <see cref="MaxParallelism"/>-bounded fan-out come from
    /// <see cref="RefreshCoalescer.RunThrottledPassAsync"/>; only the failure logging
    /// stays here, so one bad pass can never crash the loop.
    /// </summary>
    protected virtual async Task RefreshPassAsync(CancellationToken cancellationToken)
    {
        try
        {
            await _coalescer.RunThrottledPassAsync(MaxParallelism, FetchRepoAsync, cancellationToken);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "{ServiceName} activity refresh pass failed", ServiceName);
        }
    }

    /// <summary>
    /// Re-checks provider activity when fresh repo data arrives (cache load, completed
    /// rescan). Skipped while a scan is in flight — the completion notification follows
    /// right after — and while the column is disabled.
    /// </summary>
    private void OnRepoServiceChanged(object? sender, EventArgs e)
    {
        if (!_enabled || _repoService.IsBusy || _repoService.Repos.Count == 0) return;
        _ = RefreshAllAsync();
    }

    /// <summary>
    /// Template guard around a single repo fetch: while the service is disabled the
    /// core never runs and the repo settles to its unavailable state — the same end
    /// state as a failed probe — so no caller can trigger provider traffic on a
    /// disabled service.
    /// </summary>
    protected async Task<TActivity> FetchGuardedAsync(
        Repo repo,
        TActivity empty,
        Func<CancellationToken, Task<TActivity>> core,
        CancellationToken cancellationToken)
    {
        if (!IsEnabled)
        {
            MarkUnavailable(repo);
            return empty;
        }

        return await core(cancellationToken);
    }

    /// <summary>Settles the repo to its loaded-and-unavailable empty state.</summary>
    protected abstract void MarkUnavailable(Repo repo);

    /// <summary>The most recently fetched activity for the repo's folder, or the default.</summary>
    protected TActivity? CachedActivity(Repo repo)
        => repo.FolderPath is { } folder && _activityByFolder.TryGetValue(folder, out var activity)
            ? activity
            : default;

    /// <summary>Stores the repo's freshly fetched activity in the per-folder cache.</summary>
    protected void StoreActivity(Repo repo, TActivity activity)
    {
        if (repo.FolderPath is { } folder)
        {
            _activityByFolder[folder] = activity;
        }
    }
}
