using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Scans configured folders for git repositories, applying the depth and exclusion
/// rules from <see cref="ReposSettings"/>.
/// </summary>
public interface IRepoScanner
{
    /// <summary>
    /// Scans the folders configured in <paramref name="settings"/> and returns the
    /// discovered repos (one per .git folder parent), auto-tagged where applicable.
    /// </summary>
    /// <param name="settings">The repo scan configuration.</param>
    /// <returns>The scan result containing distinct, sorted repos.</returns>
    Task<RepoScanResult> ScanAsync(ReposSettings settings);
}
