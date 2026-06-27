using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Singleton owner of discovered workspace/platform data. Centralizes the shared state
/// that previously lived in static ViewModel fields (a workaround for Transient page
/// lifetimes), coordinating the scanner and cache store behind a single source of truth.
/// </summary>
public interface IWorkspaceService
{
    /// <summary>
    /// Gets the discovered workspace (solution) items.
    /// </summary>
    IReadOnlyList<WorkspaceItem> Workspaces { get; }

    /// <summary>
    /// Gets the discovered platform items.
    /// </summary>
    IReadOnlyList<WorkspaceItem> Platforms { get; }

    /// <summary>
    /// Gets a value indicating whether a scan is currently in progress.
    /// </summary>
    bool IsBusy { get; }

    /// <summary>
    /// Raised whenever the discovered data or busy state changes.
    /// </summary>
    event EventHandler? Changed;

    /// <summary>
    /// Loads cached data (if available and not already loaded) and kicks off a fresh
    /// background scan when scan folders are configured. Idempotent for the cache load.
    /// </summary>
    /// <param name="settings">The workspace scan configuration.</param>
    Task EnsureLoadedAsync(WorkspacesSettings settings);

    /// <summary>
    /// Clears cached data and reloads (cache then scan), forcing a full refresh.
    /// </summary>
    /// <param name="settings">The workspace scan configuration.</param>
    Task RefreshAsync(WorkspacesSettings settings);
}
