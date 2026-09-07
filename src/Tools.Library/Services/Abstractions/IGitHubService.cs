using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Abstracts the GitHub query surface behind the Repos page's GitHub column so the
/// ViewModels stay free of process spawning and JSON parsing. Implementations run the
/// <c>gh</c> CLI per repo (see <see cref="GitHubService"/>), push the open pull-request
/// and issue counts onto the <see cref="Repo"/> entities' runtime-only properties, and
/// cache the fetched item lists so the details dialog can open instantly. The shared
/// enable/refresh contract comes from <see cref="IRepoActivityService"/>.
/// </summary>
public interface IGitHubService : IRepoActivityService
{
    /// <summary>
    /// Re-probes a single repo and returns its open pull requests and issues (also
    /// pushed onto the <see cref="Repo"/> entity and cached). Used by the details
    /// dialog's Refresh button and as the per-repo worker of
    /// <see cref="RefreshAllAsync"/>. While the service is disabled the repo settles
    /// to its unavailable state and nothing is queried.
    /// </summary>
    Task<GitHubActivity> RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default);

    /// <summary>
    /// The most recently fetched activity for the repo, or <c>null</c> when the repo
    /// has not been probed yet (or is not a GitHub repo). Lets the details dialog render
    /// instantly from cache before its background refresh completes.
    /// </summary>
    GitHubActivity? GetCachedActivity(Repo repo);

    /// <summary>
    /// Fetches the repository's static metadata (owner, creation date, language,
    /// license, default branch, topics) via <c>gh repo view</c> and caches it per
    /// folder. Returns <c>null</c> when GitHub querying is disabled, the folder is
    /// unknown, or the probe fails / the repo is not on GitHub — the Overview sidebar
    /// hides in that case. Cached: repeated opens cost no process spawn.
    /// </summary>
    Task<GitHubRepoDetails?> GetRepoDetailsAsync(Repo repo, CancellationToken cancellationToken = default);
}
