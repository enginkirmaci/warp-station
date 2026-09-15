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

    /// <summary>
    /// Persists already-serialized cache JSON. Lets a caller serialize while holding
    /// its own consistency lock (Repo entities are UI-mutable observables, so a
    /// serialize-after-unlock can race a tag edit) and still write through the
    /// store's atomic path.
    /// </summary>
    Task SaveJsonAsync(string json);
}
