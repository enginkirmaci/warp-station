using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Singleton orchestrator that owns discovered repo data, coordinating
/// <see cref="IRepoScanner"/> and <see cref="IRepoCacheStore"/>. Holds the in-memory
/// source of truth for the Repos page (which is rebuilt per navigation as a Transient
/// VM) and merges user-defined tags across rescans by matching folder path.
/// </summary>
/// <remarks>
/// Persistence is write-behind: tag/favorite edits only dirty-flag the cache and
/// schedule a single debounced flush (400ms), so a burst of star/tag clicks collapses
/// into one full-file write instead of serializing the whole cache per click. Reads
/// always see the live in-memory state; only the file write is deferred. The service
/// implements <see cref="IDisposable"/> so the DI container's final flush on
/// <c>Host.Dispose()</c> (app shutdown) closes the deferral window.
/// </remarks>
public class RepoService : IRepoService, IDisposable
{
    private readonly IRepoScanner _scanner;
    private readonly IRepoCacheStore _cacheStore;

    private List<Repo> _repos = new();
    private bool _busy;
    private bool _cacheLoaded;
    private bool _scannedThisSession;

    /// <summary>
    /// The running (or last) session scan, recorded by <see cref="StartScan"/> so
    /// <see cref="RefreshAsync"/> can outwait the scan it kicked — the manual refresh's
    /// busy state must span the scan, while <see cref="EnsureLoadedAsync"/> fires it
    /// fire-and-forget so page navigation returns instantly.
    /// </summary>
    private Task? _scanTask;

    // Write-behind persistence state (guarded by _persistLock).
    private static readonly TimeSpan FlushDelay = TimeSpan.FromMilliseconds(400);
    private readonly object _persistLock = new();
    private bool _dirty;
    private bool _disposed;
    private Task? _flushLoop;
    private CancellationTokenSource? _flushCts;

    public RepoService(IRepoScanner scanner, IRepoCacheStore cacheStore)
    {
        _scanner = scanner;
        _cacheStore = cacheStore;
    }

    /// <inheritdoc/>
    public IReadOnlyList<Repo> Repos => _repos;

    /// <inheritdoc/>
    public bool IsBusy => _busy;

    /// <inheritdoc/>
    public IReadOnlyCollection<string> AllTags
    {
        get
        {
            var tags = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
            {
                Repo.FavoritesTag,
                RepoScanner.PlatformTag
            };
            foreach (var repo in _repos)
            {
                foreach (var tag in repo.Tags)
                    tags.Add(tag.Name);
            }
            return tags;
        }
    }

    /// <summary>
    /// The reserved tag toggled by the star affordance. Exposed as a service-level
    /// constant for callers that depend on <see cref="IRepoService"/>; mirrors
    /// <see cref="Repo.FavoritesTag"/>.
    /// </summary>
    public const string FavoritesTag = Repo.FavoritesTag;

    /// <inheritdoc/>
    public event EventHandler? Changed;

    /// <inheritdoc/>
    public event EventHandler? TagsChanged;

    /// <inheritdoc/>
    public async Task EnsureLoadedAsync(ReposSettings settings)
    {
        // Load cache once if we have no data yet, so the UI renders instantly.
        if (!_cacheLoaded && _repos.Count == 0)
        {
            var cache = await _cacheStore.LoadAsync();
            if (cache?.Repos != null)
            {
                // Re-parent loaded tags back to their repos (deserialization creates
                // RepoTag instances whose Repo back-ref may be null).
                foreach (var repo in cache.Repos)
                {
                    var names = repo.Tags.Select(t => t.Name).ToList();
                    repo.Tags.Clear();
                    foreach (var name in names)
                        repo.AddTag(name);
                }
                _repos = cache.Repos;
                _cacheLoaded = true;
                RaiseChanged();
            }
        }

        // Scan once per app session: the background scan is disk-bound (folder walk
        // plus a per-repo solution file lookup), so re-running it on every navigation to
        // the Repos page would hammer the file system and re-trigger a full git status
        // pass each time. Later navigations serve the in-memory/cache data; the manual
        // Refresh command (RefreshAsync) forces a rescan. A failed scan leaves the flag
        // unset so the next navigation retries.
        if (!_scannedThisSession && settings.RepoScanFolders?.Any() == true)
        {
            StartScan(settings);
        }
    }

    /// <inheritdoc/>
    public async Task RefreshAsync(ReposSettings settings)
    {
        // Keep the in-memory repos so the UI does not blank out during a manual refresh;
        // the scan replaces them once it completes.
        _cacheLoaded = false;
        _scannedThisSession = false;
        RaiseChanged();
        await EnsureLoadedAsync(settings);

        // EnsureLoadedAsync kicks the rescan fire-and-forget (page navigations must not
        // wait on it); the manual refresh has to — its caller's busy state (the Repos
        // page's Refresh button) spans the whole scan + git-status cycle, so the data on
        // screen is final the moment the busy state clears. The task is current here:
        // this call just started the scan, or an earlier one is still settling. It never
        // faults — ScanCoreAsync catches, logs and settles internally.
        if (_scanTask is { IsCompleted: false })
        {
            await _scanTask;
        }
    }

    /// <inheritdoc/>
    public Task AddTagAsync(Repo repo, string tag)
    {
        repo.AddTag(tag);
        ScheduleSave();
        RaiseTagsChanged();
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public Task RemoveTagAsync(Repo repo, string tag)
    {
        if (!repo.RemoveTag(tag)) return Task.CompletedTask;
        ScheduleSave();
        RaiseTagsChanged();
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public Task ToggleFavoriteAsync(Repo repo)
    {
        if (!repo.RemoveTag(FavoritesTag))
            repo.AddTag(FavoritesTag);

        ScheduleSave();
        RaiseTagsChanged();
        return Task.CompletedTask;
    }

    /// <summary>
    /// Claims the scan slot and starts the session scan, recording the running task in
    /// <see cref="_scanTask"/> for <see cref="RefreshAsync"/>. A call arriving while a
    /// scan is in flight is a no-op that leaves the field alone — the in-flight task
    /// stays the current one (assigning a rejected no-op over it would let a waiting
    /// refresh return before the real scan finished).
    /// </summary>
    private void StartScan(ReposSettings settings)
    {
        if (_busy) return;
        _busy = true;
        RaiseChanged();
        _scanTask = ScanCoreAsync(settings);
    }

    /// <summary>
    /// The scan body behind <see cref="_scanTask"/>. Never faults: failures are logged
    /// and the finally block settles <see cref="_busy"/> and raises <c>Changed</c> —
    /// the raise that tells the git-status services to re-probe the fresh list.
    /// </summary>
    private async Task ScanCoreAsync(ReposSettings settings)
    {
        try
        {
            // Listing scan: omitting the depth makes the scanner non-recursive (direct
            // children of each scan root only). Deep scanning is the Add Repositories
            // dialog's job.
            var result = await _scanner.ScanAsync(settings);
            var scanned = result.Repos;

            // Merge user-defined tags from the previous cache/state: anything the user
            // added by hand is carried over to the freshly-scanned repo for the same
            // folder. Auto-tags (platform) are recomputed by the scanner and therefore
            // excluded from the carry-over so a renamed folder does not retain a stale
            // platform tag.
            var previousByPath = _repos
                .Where(r => r.FolderPath is not null)
                .ToDictionary(r => r.FolderPath!);

            foreach (var repo in scanned)
            {
                if (repo.FolderPath is null) continue;
                if (previousByPath.TryGetValue(repo.FolderPath, out var prev))
                {
                    foreach (var tag in prev.Tags)
                    {
                        if (string.Equals(tag.Name, RepoScanner.PlatformTag, StringComparison.OrdinalIgnoreCase))
                            continue;
                        repo.AddTag(tag.Name);
                    }

                    // Carry over the last-known git status so a rescan of an unchanged
                    // repo does not flip its card back to the "checking…" placeholder;
                    // the status service re-probes in the background anyway.
                    repo.GitBranchName = prev.GitBranchName;
                    repo.GitModifiedCount = prev.GitModifiedCount;
                    repo.GitToPushCount = prev.GitToPushCount;
                    repo.GitToPullCount = prev.GitToPullCount;
                    repo.GitStatusLoaded = prev.GitStatusLoaded;

                    // Same carry-over for the GitHub column cells.
                    repo.GitHubRepoUrl = prev.GitHubRepoUrl;
                    repo.GitHubPrCount = prev.GitHubPrCount;
                    repo.GitHubIssueCount = prev.GitHubIssueCount;
                    repo.GitHubLoaded = prev.GitHubLoaded;
                    repo.GitHubAvailable = prev.GitHubAvailable;
                }
            }

            _repos = scanned;
            _scannedThisSession = true;

            ScheduleSave();

            // No RaiseChanged here: the start raise above already flipped _busy to true,
            // and the finally raise below signals both data-ready and _busy=false in one
            // go. Consumers already handle both — GitStatusService skips while busy and
            // re-checks on completion; ReposViewModel debounces its rebuild. A third raise
            // mid-scan would only add an extra redundant rebuild + status pass.
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error scanning repos");
        }
        finally
        {
            _busy = false;
            RaiseChanged();
        }
    }

    /// <summary>
    /// Marks the cache dirty and ensures a single debounced flush is scheduled. Burst
    /// edits inside one <see cref="FlushDelay"/> window (e.g. rapid star/tag clicks)
    /// collapse into one file write; edits landing while a flush is already writing
    /// re-arm the dirty flag and are picked up by the next loop iteration. Never
    /// touches the file system on the toggling caller's path.
    /// </summary>
    private void ScheduleSave()
    {
        lock (_persistLock)
        {
            if (_disposed) return;
            _dirty = true;
            if (_flushLoop != null) return; // a flush loop is pending or writing; it will observe _dirty

            var cts = new CancellationTokenSource();
            _flushCts = cts;
            _flushLoop = Task.Run(() => FlushLoopAsync(cts.Token));
        }
    }

    /// <summary>
    /// The write-behind loop: waits out the debounce window, then writes the full cache
    /// exactly as the previous per-call save did (same <see cref="RepoCache"/> payload
    /// through the same <see cref="IRepoCacheStore"/> path, so file format and location
    /// are unchanged). Exits when quiescent; failures leave the dirty flag set so the
    /// next trigger retries, and never propagate (the loop is fire-and-forget).
    /// </summary>
    private async Task FlushLoopAsync(CancellationToken ct)
    {
        while (true)
        {
            try
            {
                await Task.Delay(FlushDelay, ct);
            }
            catch (OperationCanceledException)
            {
                // Dispose cancelled the debounce; Dispose owns the final flush.
                return;
            }

            RepoCache snapshot;
            lock (_persistLock)
            {
                if (_disposed) return;
                if (!_dirty)
                {
                    // Quiescent: hand the loop slot back so the next trigger starts fresh.
                    _flushLoop = null;
                    _flushCts?.Dispose();
                    _flushCts = null;
                    return;
                }
                _dirty = false;
                // Same shallow copy the old synchronous save built; the store serializes
                // this identical shape, so what is persisted does not change.
                snapshot = new RepoCache { Repos = _repos.ToList() };
            }

            try
            {
                await _cacheStore.SaveAsync(snapshot);
            }
            catch (Exception ex)
            {
                // The store normally logs and swallows its own IO errors; this guards the
                // fire-and-forget loop itself. Re-arm and release the slot so the next
                // trigger (edit or shutdown flush) rewrites the full current state.
                Log.Logger.Error(ex, "Error flushing repo cache");
                lock (_persistLock)
                {
                    _dirty = true;
                    _flushLoop = null;
                    _flushCts?.Dispose();
                    _flushCts = null;
                    return;
                }
            }
        }
    }

    /// <summary>
    /// Final flush closing the write-behind window on app exit. Wired up by DI: the
    /// service is a singleton, so the host's <c>Host.Dispose()</c> in
    /// <c>App.OnShutdownRequested</c> calls this during shutdown.
    /// </summary>
    public void Dispose()
    {
        Task? loop;
        lock (_persistLock)
        {
            if (_disposed) return;
            _disposed = true;
            loop = _flushLoop;
            _flushLoop = null;
            _flushCts?.Cancel();
        }

        try
        {
            // Wait out any in-flight debounce/write so the final flush below cannot race it.
            // Cancellation bounds the wait to the remaining file IO (~milliseconds).
            loop?.GetAwaiter().GetResult();
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error waiting for pending repo cache flush");
        }

        RepoCache? pending;
        lock (_persistLock)
        {
            pending = null;
            if (_dirty)
            {
                _dirty = false;
                pending = new RepoCache { Repos = _repos.ToList() };
            }
            _flushCts?.Dispose();
            _flushCts = null;
        }

        if (pending == null) return;

        try
        {
            // Host.Dispose() runs on the UI thread during shutdown; hop to the thread pool
            // so the store's awaited file IO cannot deadlock on the blocked UI context.
            Task.Run(() => _cacheStore.SaveAsync(pending)).GetAwaiter().GetResult();
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error flushing repo cache on shutdown");
        }
    }

    private void RaiseChanged() => Changed?.Invoke(this, EventArgs.Empty);

    private void RaiseTagsChanged() => TagsChanged?.Invoke(this, EventArgs.Empty);
}
