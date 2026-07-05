using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Singleton owner of discovered repo data. Centralizes the shared state that would
/// otherwise live in static ViewModel fields (a workaround for Transient page
/// lifetimes), coordinating the scanner and cache store behind a single source of
/// truth. Also owns user-defined tags, merging them across rescans by folder path.
/// </summary>
public interface IRepoService
{
    /// <summary>
    /// Gets the discovered repos.
    /// </summary>
    IReadOnlyList<Repo> Repos { get; }

    /// <summary>
    /// Gets a value indicating whether a scan is currently in progress.
    /// </summary>
    bool IsBusy { get; }

    /// <summary>
    /// Gets the distinct set of all tags currently in use across repos, plus the
    /// reserved <c>favorites</c> and <c>platform</c> tags (so the filter list is
    /// stable even when no repo carries them yet).
    /// </summary>
    IReadOnlyCollection<string> AllTags { get; }

    /// <summary>
    /// Raised whenever the discovered data, tags, or busy state changes.
    /// </summary>
    event EventHandler? Changed;

    /// <summary>
    /// Loads cached data (if available and not already loaded) and kicks off a fresh
    /// background scan when scan folders are configured. Idempotent for the cache load.
    /// </summary>
    /// <param name="settings">The repo scan configuration.</param>
    Task EnsureLoadedAsync(ReposSettings settings);

    /// <summary>
    /// Clears cached data and reloads (cache then scan), forcing a full refresh.
    /// </summary>
    /// <param name="settings">The repo scan configuration.</param>
    Task RefreshAsync(ReposSettings settings);

    /// <summary>
    /// Adds a tag to a repo (no-op if already present) and persists the change.
    /// </summary>
    Task AddTagAsync(Repo repo, string tag);

    /// <summary>
    /// Removes a tag from a repo (no-op if absent) and persists the change.
    /// </summary>
    Task RemoveTagAsync(Repo repo, string tag);

    /// <summary>
    /// Toggles the reserved <c>favorites</c> tag on a repo and persists the change.
    /// </summary>
    Task ToggleFavoriteAsync(Repo repo);
}
