using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Scans configured folders for git repositories, applying the exclusion rules from
/// <see cref="ReposSettings"/>. The scan depth is chosen per call: the main repo
/// listing scans non-recursively, while the Add Repositories dialog passes its
/// configured depth to also discover deeper repos.
/// </summary>
public interface IRepoScanner
{
    /// <summary>
    /// Scans the folders configured in <paramref name="settings"/> and returns the
    /// discovered repos (one per .git folder parent), auto-tagged where applicable.
    /// </summary>
    /// <param name="settings">The repo scan configuration.</param>
    /// <param name="maxDepth">
    /// The maximum folder depth to walk below each scan root (1 = direct children
    /// only). Omit to scan non-recursively — the main repo listing's behavior; the
    /// Add Repositories dialog passes its configured scan depth to search deeper.
    /// </param>
    /// <returns>The scan result containing distinct, sorted repos.</returns>
    Task<RepoScanResult> ScanAsync(ReposSettings settings, int? maxDepth = null);
}
