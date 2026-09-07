namespace Tools.Library.Entities;

/// <summary>
/// One recent commit of a repo, parsed from <c>git log</c> for the bottom bar's
/// Changes tab. Carries the full hash (the commit-id click copies it) plus the
/// display-ready subject, author and commit date.
/// </summary>
public sealed record GitCommitInfo(
    string Hash,
    string Subject,
    string? Author,
    DateTimeOffset Date)
{
    /// <summary>Seven-char display form of the hash for the Recent Commits column.</summary>
    public string ShortHash => Hash.Length <= 7 ? Hash : Hash[..7];
    /// <summary>Relative age label for the commit ("2h ago"); null when unparsable.</summary>
    public string? RelativeTime => FormatRelative(Date);

    /// <summary>
    /// Up-to-two-letter initials for the History row's avatar circle — first letters of
    /// the author's first and last name part ("Engin Kirmaci" → "EK"), or "?" when the
    /// author is unknown.
    /// </summary>
    public string Initials
    {
        get
        {
            if (string.IsNullOrWhiteSpace(Author)) return "?";
            var parts = Author.Split(' ', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            return parts.Length == 1
                ? parts[0][..1].ToUpperInvariant()
                : $"{char.ToUpperInvariant(parts[0][0])}{char.ToUpperInvariant(parts[^1][0])}";
        }
    }

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
