using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Common contract of the hosting-provider activity services behind the Repos page's
/// optional columns (GitHub via the <c>gh</c> CLI, Azure DevOps via the REST API):
/// settings-driven enable/configure and a coalescing background refresh pass, with the
/// guarantee that a disabled service never queries. Provider-specific surfaces — the
/// typed per-repo fetch and the cached activity — stay on the derived interfaces
/// (<see cref="IGitHubService"/>, <see cref="IAzureDevOpsService"/>); the shared
/// machinery lives in the <c>RepoActivityServiceBase</c> template they derive from.
/// </summary>
public interface IRepoActivityService
{
    /// <summary>
    /// Whether querying is currently enabled: the settings' column flag (via
    /// <see cref="Configure"/>) <em>and</em> the provider being operational (a
    /// resolvable <c>gh</c>; a usable personal access token). When
    /// <see langword="false"/>, full refreshes no-op and per-repo fetches settle their
    /// repo to the unavailable state instead — nothing is ever queried.
    /// </summary>
    bool IsEnabled { get; }

    /// <summary>
    /// Applies the current repo settings: the column's enabled flag plus the
    /// provider-specific executable/token. Called by the page whenever settings load or
    /// are saved.
    /// </summary>
    void Configure(ReposSettings settings);

    /// <summary>
    /// Refreshes the activity of every known repo in the background. Re-entrant:
    /// concurrent calls are coalesced the same way <c>IGitStatusService</c> coalesces
    /// refreshes. Never throws; repos whose probe fails are marked loaded and
    /// unavailable so the column cell settles to its empty state. No-op (and queries
    /// nothing) while <see cref="IsEnabled"/> is <see langword="false"/>.
    /// </summary>
    Task RefreshAllAsync(CancellationToken cancellationToken = default);
}
