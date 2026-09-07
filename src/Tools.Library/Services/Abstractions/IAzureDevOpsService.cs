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
/// instantly. The shared enable/refresh contract comes from
/// <see cref="IRepoActivityService"/>.
/// </summary>
public interface IAzureDevOpsService : IRepoActivityService
{
    /// <summary>
    /// Re-probes a single repo and returns its open pull requests, open work items and
    /// recent pipeline runs (also pushed onto the <see cref="Repo"/> entity and cached).
    /// Used by the details dialog's Refresh button and as the per-repo worker of
    /// <see cref="RefreshAllAsync"/>. While the service is disabled the repo settles
    /// to its unavailable state and nothing is queried.
    /// </summary>
    Task<AzureDevOpsActivity> RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// The most recently fetched activity for the repo, or <c>null</c> when the repo
    /// has not been probed yet (or is not hosted on Azure DevOps). Lets the details
    /// dialog render instantly from cache before its background refresh completes.
    /// </summary>
    AzureDevOpsActivity? GetCachedActivity(Repo repo);
}
