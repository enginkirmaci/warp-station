using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// The open pull requests and issues fetched for one repo, in dialog display order:
/// pull requests first, then issues.
/// </summary>
public sealed record GitHubActivity(
    IReadOnlyList<GitHubItem> PullRequests,
    IReadOnlyList<GitHubItem> Issues)
{
    public static readonly GitHubActivity Empty = new([], []);
}
