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

        // Always scan in background to keep data fresh, even if we loaded from cache.
        if (settings.RepoScanFolders?.Any() == true)
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
        RaiseChanged();
        await EnsureLoadedAsync(settings);
    }

    /// <inheritdoc/>
    public async Task AddTagAsync(Repo repo, string tag)
    {
        repo.AddTag(tag);
        await SaveAsync();
        RaiseChanged();
    }

    /// <inheritdoc/>
    public async Task RemoveTagAsync(Repo repo, string tag)
    {
        if (!repo.RemoveTag(tag)) return;
        await SaveAsync();
        RaiseChanged();
    }

    /// <inheritdoc/>
    public async Task ToggleFavoriteAsync(Repo repo)
    {
        if (!repo.RemoveTag(FavoritesTag))
            repo.AddTag(FavoritesTag);

        await SaveAsync();
        RaiseChanged();
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
                }
            }

            _repos = scanned;

            await SaveAsync();

            RaiseChanged();
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
}
