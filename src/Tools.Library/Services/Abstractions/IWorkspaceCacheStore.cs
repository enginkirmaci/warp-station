using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Persists discovered workspaces and platforms to a cache file so the UI can render
/// instantly on startup while a fresh scan runs in the background.
/// </summary>
public interface IWorkspaceCacheStore
{
    /// <summary>
    /// Loads the cached workspace data, or <c>null</c> if no cache file exists.
    /// </summary>
    Task<WorkspaceCache?> LoadAsync();

    /// <summary>
    /// Saves the given cache data, overwriting any existing cache file.
    /// </summary>
    Task SaveAsync(WorkspaceCache cache);
}
