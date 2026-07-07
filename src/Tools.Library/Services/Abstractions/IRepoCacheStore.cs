using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Persists discovered repos (and their user-defined tags) to a cache file so the UI
/// can render instantly on startup while a fresh scan runs in the background.
/// </summary>
public interface IRepoCacheStore
{
    /// <summary>
    /// Loads the cached repo data, or <c>null</c> if no cache file exists.
    /// </summary>
    Task<RepoCache?> LoadAsync();

    /// <summary>
    /// Saves the given cache data, overwriting any existing cache file.
    /// </summary>
    Task SaveAsync(RepoCache cache);
}
