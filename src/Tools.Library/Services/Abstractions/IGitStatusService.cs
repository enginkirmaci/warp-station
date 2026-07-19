using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Singleton that computes local git status (branch, modified count, ahead/behind) for
/// every discovered repo and pushes the results onto the <see cref="Repo"/> entities'
/// runtime-only properties, which the repo cards bind directly. All work happens on
/// background threads via the <c>git</c> CLI with redirected output — calling it never
/// blocks the UI. Refreshes are also triggered automatically whenever
/// <see cref="IRepoService"/> reports new scan data.
/// </summary>
public interface IGitStatusService
{
    /// <summary>
    /// Refreshes the git status of every known repo in the background. Re-entrant:
    /// concurrent calls are coalesced — a call arriving while a refresh is running marks
    /// a pending pass that runs once the current one finishes. Never throws; repos whose
    /// check fails are marked loaded with zeroed counts so the UI stops showing the
    /// "checking…" placeholder.
    /// </summary>
    /// <param name="cancellationToken">Cancels the refresh loop.</param>
    Task RefreshAllAsync(CancellationToken cancellationToken = default);
}
