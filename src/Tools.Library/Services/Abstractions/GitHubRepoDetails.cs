using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Static repository metadata for one repo as reported by <c>gh repo view</c> — the
/// owner, creation date, language, license, default branch and topic list shown in the
/// bottom bar's Overview sidebar. Every field is nullable: a non-GitHub repo (or a gh
/// failure) yields <see cref="Empty"/>-shaped data and the UI hides the panel.
/// </summary>
public sealed record GitHubRepoDetails(
    string? Owner,
    DateTimeOffset? CreatedAt,
    string? Language,
    string? License,
    string? DefaultBranch,
    IReadOnlyList<string> Topics,
    string? Url)
{
    public static readonly GitHubRepoDetails Empty = new(null, null, null, null, null, [], null);

    /// <summary>Whether any topic chips show.</summary>
    public bool HasTopics => Topics.Count > 0;

    /// <summary>Whether the panel has anything to show at all.</summary>
    public bool HasContent => Owner is not null || CreatedAt is not null || Language is not null
        || License is not null || DefaultBranch is not null || Topics.Count > 0;
}
