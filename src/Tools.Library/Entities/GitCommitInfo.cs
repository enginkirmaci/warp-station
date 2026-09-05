namespace Tools.Library.Entities;

/// <summary>
/// One recent commit of a repo, parsed from <c>git log</c> for the bottom bar's Git
/// tab. Carries the display-ready short hash, subject, author and commit date.
/// </summary>
public sealed record GitCommitInfo(
    string ShortHash,
    string Subject,
    string? Author,
    DateTimeOffset Date)
{
    /// <summary>Relative age label for the commit ("2h ago"); null when unparsable.</summary>
    public string? RelativeTime => FormatRelative(Date);

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
