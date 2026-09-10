namespace Tools.Library.Services;

/// <summary>
/// Pure URL helpers for the clone flow — the only git surface that starts from a URL
/// the user typed instead of an existing repo. Nothing here runs git or touches the
/// filesystem.
/// </summary>
public static class GitUrlParser
{
    /// <summary>
    /// Derives the destination folder name for <c>git clone &lt;url&gt;</c>: the last
    /// path segment with a trailing <c>.git</c> stripped — the name git itself would
    /// pick. Handles the <c>git@host:owner/repo.git</c> SCP shape (the segment after
    /// the final slash), a trailing slash, and bare local paths. Returns null when the
    /// URL has no usable final segment (empty, or a lone "https://" with nothing
    /// after it).
    /// </summary>
    public static string? DeriveRepoName(string? url)
    {
        if (string.IsNullOrWhiteSpace(url))
        {
            return null;
        }

        var trimmed = url.Trim().TrimEnd('/');
        if (trimmed.Length == 0)
        {
            return null;
        }

        // The scheme ends at "://"; a URL without one is a local path or an SCP-style
        // host:path — both keep their full text for the segment split below.
        var schemeEnd = trimmed.IndexOf("://", StringComparison.Ordinal);
        var withoutScheme = schemeEnd >= 0 ? trimmed[(schemeEnd + 3)..] : trimmed;

        // The name is the last segment after '/' (covers HTTPS, SSH and SCP paths);
        // a pathless URL falls back to the segment after the last ':' — the SCP
        // host:repo shape (a port colon only appears inside ssh:// URLs, which
        // always carry a path).
        var lastSlash = withoutScheme.LastIndexOf('/');
        var segment = lastSlash >= 0
            ? withoutScheme[(lastSlash + 1)..]
            : withoutScheme;
        if (lastSlash < 0)
        {
            var lastColon = segment.LastIndexOf(':');
            if (lastColon >= 0)
            {
                segment = segment[(lastColon + 1)..];
            }
        }

        if (segment.Length == 0 || segment is "." or "..")
        {
            return null;
        }

        return segment.EndsWith(".git", StringComparison.OrdinalIgnoreCase)
            ? segment[..^4]
            : segment;
    }
}
