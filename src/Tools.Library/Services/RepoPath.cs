namespace Tools.Library.Services;

/// <summary>
/// Equality helpers for repo folder paths. The same folder can be spelled with
/// different casing on Windows and with or without a trailing separator anywhere, so
/// "is this repo already tracked / is this folder already a scan root" comparisons go
/// through here instead of raw string equality.
/// </summary>
public static class RepoPath
{
    /// <summary>
    /// Comparer for exact repo folder path equality on the running platform: paths are
    /// case-insensitive on Windows and case-sensitive elsewhere.
    /// </summary>
    public static StringComparer Comparer => OperatingSystem.IsWindows()
        ? StringComparer.OrdinalIgnoreCase
        : StringComparer.Ordinal;

    /// <summary>
    /// Whether two repo folder paths denote the same folder on the running platform
    /// (trailing separators and surrounding whitespace are ignored).
    /// </summary>
    public static bool SamePath(string? a, string? b)
    {
        if (string.IsNullOrWhiteSpace(a) || string.IsNullOrWhiteSpace(b)) return false;

        var normalizedA = Path.TrimEndingDirectorySeparator(a.Trim());
        var normalizedB = Path.TrimEndingDirectorySeparator(b.Trim());
        return Comparer.Equals(normalizedA, normalizedB);
    }
}
