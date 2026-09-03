using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Abstracts the Azure DevOps query surface behind the Repos page's Azure DevOps column
/// so the ViewModels stay free of HTTP, remote-URL parsing and JSON handling.
/// Implementations resolve each repo's Azure DevOps organization/project/repo from its
/// git remote (no extra CLI needed — unlike <see cref="IGitHubService"/>), call the REST
/// API with the configured personal access token, push the active pull-request, open
/// work-item and latest-pipeline-run summary onto the <see cref="Repo"/> entities'
/// runtime-only properties, and cache the fetched lists so the details dialog can open
/// instantly.
/// </summary>
public interface IAzureDevOpsService
{
    /// <summary>
    /// Whether Azure DevOps querying is currently enabled (the column flag via
    /// <see cref="Configure"/> <em>and</em> a usable token). When
    /// <see langword="false"/>, all refreshes no-op — the column shows nothing and no
    /// request is ever sent.
    /// </summary>
    bool IsEnabled { get; }

    /// <summary>
    /// Applies the current repo settings: the column's enabled flag and the configured
    /// personal access token. Called by the page whenever settings load or are saved.
    /// </summary>
    void Configure(Configuration.ReposSettings settings);

    /// <summary>
    /// Refreshes the Azure DevOps activity of every known repo in the background.
    /// Re-entrant: concurrent calls are coalesced the same way
    /// <c>IGitStatusService</c> coalesces refreshes. Never throws; repos whose probe
    /// fails are marked loaded and unavailable so the column cell settles to its empty
    /// state. No-op (and sends nothing) while <see cref="IsEnabled"/> is
    /// <see langword="false"/>.
    /// </summary>
    Task RefreshAllAsync(CancellationToken cancellationToken = default);

    /// <summary>
    /// Re-probes a single repo and returns its open pull requests, open work items and
    /// recent pipeline runs (also pushed onto the <see cref="Repo"/> entity and cached).
    /// Used by the details dialog's Refresh button and as the per-repo worker of
    /// <see cref="RefreshAllAsync"/>. Works even when the column is disabled, so a
    /// dialog opened while disabled still refreshes on request.
    /// </summary>
    Task<AzureDevOpsActivity> RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// The most recently fetched activity for the repo, or <c>null</c> when the repo
    /// has not been probed yet (or is not hosted on Azure DevOps). Lets the details
    /// dialog render instantly from cache before its background refresh completes.
    /// </summary>
    AzureDevOpsActivity? GetCachedActivity(Repo repo);
}
