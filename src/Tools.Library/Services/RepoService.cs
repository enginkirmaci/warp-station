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
public class RepoService : IRepoService
{
    private readonly IRepoScanner _scanner;
    private readonly IRepoCacheStore _cacheStore;

    private List<Repo> _repos = new();
    private bool _busy;
    private bool _cacheLoaded;
    private bool _scannedThisSession;

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

        // Scan once per app session: the background scan is disk-bound (recursive walk
        // plus a per-repo solution file lookup), so re-running it on every navigation to
        // the Repos page would hammer the file system and re-trigger a full git status
        // pass each time. Later navigations serve the in-memory/cache data; the manual
        // Refresh command (RefreshAsync) forces a rescan. A failed scan leaves the flag
        // unset so the next navigation retries.
        if (!_scannedThisSession && settings.RepoScanFolders?.Any() == true)
        {
            _ = ScanAsync(settings);
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
    }

    /// <inheritdoc/>
    public async Task AddTagAsync(Repo repo, string tag)
    {
        repo.AddTag(tag);
        await SaveAsync();
        RaiseTagsChanged();
    }

    /// <inheritdoc/>
    public async Task RemoveTagAsync(Repo repo, string tag)
    {
        if (!repo.RemoveTag(tag)) return;
        await SaveAsync();
        RaiseTagsChanged();
    }

    /// <inheritdoc/>
    public async Task ToggleFavoriteAsync(Repo repo)
    {
        if (!repo.RemoveTag(FavoritesTag))
            repo.AddTag(FavoritesTag);

        await SaveAsync();
        RaiseTagsChanged();
    }

    private async Task ScanAsync(ReposSettings settings)
    {
        if (_busy) return;
        _busy = true;
        RaiseChanged();

        try
        {
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
                }
            }

            _repos = scanned;
            _scannedThisSession = true;

            await SaveAsync();

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

    private async Task SaveAsync()
    {
        await _cacheStore.SaveAsync(new RepoCache
        {
            Repos = _repos.ToList()
        });
    }

    private void RaiseChanged() => Changed?.Invoke(this, EventArgs.Empty);

    private void RaiseTagsChanged() => TagsChanged?.Invoke(this, EventArgs.Empty);
}
