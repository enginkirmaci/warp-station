namespace Tools.Library.Formatters;

/// <summary>
/// Formats timestamps as the compact relative age labels shared by the Repos table,
/// the bottom bar's GitHub and Changes panels and the Azure DevOps pipeline tooltip:
/// the most significant unit only, no rounding up across unit boundaries
/// (<c>just now</c>, <c>5m ago</c>, <c>2h ago</c>, <c>1d ago</c>, <c>3w ago</c>,
/// <c>1mo ago</c>, <c>1y ago</c>). Uses the wall-clock difference to
/// <see cref="DateTimeOffset.Now"/>, so the label is unaffected by the timestamp's
/// UTC offset; future timestamps (clock skew) clamp to <c>just now</c>.
/// </summary>
public static class RelativeTime
{
    /// <summary>
    /// Formats the age of <paramref name="at"/> as a compact relative label
    /// (<c>just now</c>, <c>5m ago</c>, <c>2h ago</c>, <c>1d ago</c>, <c>3w ago</c>,
    /// <c>1mo ago</c>, <c>1y ago</c>).
    /// </summary>
    /// <param name="at">The timestamp to format the age of.</param>
    /// <returns>The relative age label.</returns>
    public static string Format(DateTimeOffset at)
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
