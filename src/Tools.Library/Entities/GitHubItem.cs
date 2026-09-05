namespace Tools.Library.Entities;

/// <summary>
/// One open pull request or issue as reported by the <c>gh</c> CLI, carried into the
/// bottom bar's panels and the Overview tab. <see cref="IsDraft"/>,
/// <see cref="HeadRef"/>, <see cref="BaseRef"/> and <see cref="ReviewDecision"/> are
/// only meaningful for pull requests.
/// </summary>
public sealed record GitHubItem(
    int Number,
    string Title,
    string Url,
    string? Author,
    IReadOnlyList<string> Labels,
    bool IsDraft,
    string? HeadRef = null,
    string? BaseRef = null,
    string? ReviewDecision = null,
    DateTimeOffset? UpdatedAt = null)
{
    /// <summary>Whether any label chips show under the item's title.</summary>
    public bool HasLabels => Labels.Count > 0;

    /// <summary>
    /// The pull request's branch flow line ("feature/linux → main"); null for issues
    /// and pull requests whose head ref the CLI did not report.
    /// </summary>
    public string? BranchSummary => HeadRef is null ? null : $"{HeadRef} → {BaseRef ?? "main"}";

    /// <summary>
    /// Right-hand state chip text for a pull request: "Draft" while a draft, "Approved"
    /// once a review approved it, "Review" otherwise (open, awaiting review).
    /// </summary>
    public string StateChip => IsDraft ? "Draft"
        : string.Equals(ReviewDecision, "APPROVED", StringComparison.OrdinalIgnoreCase) ? "Approved"
        : "Review";

    /// <summary>Age text from the item's last update ("2h ago"); null when unknown.</summary>
    public string? AgeText => UpdatedAt is { } at ? FormatRelative(at) : null;

    /// <summary>
    /// Relative age label for a timestamp (<c>just now</c>, <c>5m ago</c>, <c>2h ago</c>,
    /// <c>1d ago</c>, <c>3w ago</c>, <c>1mo ago</c>, <c>1y ago</c>).
    /// </summary>
    private static string FormatRelative(DateTimeOffset at)
    {
        var span = DateTimeOffset.Now - at;
        var minutes = (int)(span.Ticks < 0 ? 0 : span.TotalMinutes);
        return minutes switch
        {
            < 1 => "just now",
            < 60 => $"{minutes}m ago",
            _ when minutes < 60 * 24 => $"{minutes / 60}h ago",
            _ when minutes < 60 * 24 * 7 => $"{minutes / (60 * 24)}d ago",
            _ when minutes < 60 * 24 * 30 => $"{minutes / (60 * 24 * 7)}w ago",
            _ when minutes < 60 * 24 * 365 => $"{minutes / (60 * 24 * 30)}mo ago",
            _ => $"{minutes / (60 * 24 * 365)}y ago",
        };
    }
}
