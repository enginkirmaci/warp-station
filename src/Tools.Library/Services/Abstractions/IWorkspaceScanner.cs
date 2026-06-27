using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Scans configured folders for solution files and platform folders, applying the
/// depth and exclusion rules from <see cref="WorkspacesSettings"/>.
/// </summary>
public interface IWorkspaceScanner
{
    /// <summary>
    /// Scans the folders configured in <paramref name="settings"/> and returns the
    /// discovered workspaces and platforms.
    /// </summary>
    /// <param name="settings">The workspace scan configuration.</param>
    /// <returns>The scan result containing distinct, sorted workspaces and platforms.</returns>
    Task<WorkspaceScanResult> ScanAsync(WorkspacesSettings settings);
}
