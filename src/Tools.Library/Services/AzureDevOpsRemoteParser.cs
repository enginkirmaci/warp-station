using Serilog;

namespace Tools.Library.Services;

/// <summary>
/// Parses a repo's Azure DevOps coordinates out of its git remote: the API base up to
/// the organization (<c>https://dev.azure.com/{org}</c>) plus the project and
/// repository segments. Recognizes the public HTTPS/SSH/legacy hosts and — when a
/// custom server URL is configured in the settings — company-hosted Azure DevOps
/// Server remotes too. Pure functions; <see cref="AzureDevOpsService"/> owns the
/// per-folder memoization and the settings lifecycle.
/// </summary>
internal static class AzureDevOpsRemoteParser
{
    /// <summary>
    /// The Azure DevOps coordinates parsed from a repo's git remote: the API base up to
    /// the organization (<c>https://dev.azure.com/{org}</c>) and the project and
    /// repository segments, already URL-escaped.
    /// </summary>
    internal sealed record AzureDevOpsRemote(string BaseUrl, string Project, string Repository);

    /// <summary>
    /// Reads a remote's URL from <c>.git/config</c>: <paramref name="name"/> picks one
    /// remote; <see langword="null"/> returns the first remote section that has a URL.
    /// </summary>
    public static string? ReadRemoteUrl(string folderPath, string? name)
    {
        try
        {
            var configPath = Path.Combine(folderPath, ".git", "config");
            if (!File.Exists(configPath)) return null;

            var inSection = false;
            foreach (var rawLine in File.ReadLines(configPath))
            {
                var line = rawLine.Trim();
                if (line.StartsWith('['))
                {
                    inSection = name is not null
                        ? line.Equals($"[remote \"{name}\"]", StringComparison.OrdinalIgnoreCase)
                        : line.StartsWith("[remote", StringComparison.OrdinalIgnoreCase);
                    continue;
                }
                if (!inSection) continue;

                var eq = line.IndexOf('=');
                if (eq <= 0) continue;
                var key = line[..eq].Trim();
                if (!key.Equals("url", StringComparison.OrdinalIgnoreCase)) continue;

                var url = line[(eq + 1)..].Trim();
                if (url.Length > 0) return url;
            }
            return null;
        }
        catch (Exception ex)
        {
            Log.Logger.Debug(ex, "Failed reading git config for {FolderPath}", folderPath);
            return null;
        }
    }

    /// <summary>
    /// Trims the settings' custom server URL and adds the https scheme when it was
    /// written bare (<c>devops.company.com</c>); <see langword="null"/> when empty.
    /// </summary>
    public static string? NormalizeServerUrl(string? value)
    {
        var trimmed = value?.Trim().TrimEnd('/');
        if (string.IsNullOrEmpty(trimmed)) return null;
        return trimmed.StartsWith("http://", StringComparison.OrdinalIgnoreCase)
            || trimmed.StartsWith("https://", StringComparison.OrdinalIgnoreCase)
            ? trimmed
            : $"https://{trimmed}";
    }

    /// <summary>
    /// Parses the repo's <c>.git/config</c> remote URL for Azure DevOps coordinates —
    /// no git process needed, and a GUI session's minimal PATH cannot break it.
    /// Recognizes <c>https://dev.azure.com/{org}/{project}/_git/{repo}</c>,
    /// the legacy <c>https://{org}.visualstudio.com/{project}/_git/{repo}</c> host,
    /// the <c>git@ssh.dev.azure.com:v3/{org}/{project}/{repo}</c> SSH form and — when a
    /// custom server URL is configured in the settings — remotes under that host too
    /// (company-hosted Azure DevOps Server, with or without a collection/app-tier path).
    /// </summary>
    public static AzureDevOpsRemote? ParseUrl(string url, string? customBaseUrl = null)
    {
        // SSH: git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
        var sshMarker = "ssh.dev.azure.com:v3/";
        var sshIndex = url.IndexOf(sshMarker, StringComparison.OrdinalIgnoreCase);
        if (sshIndex >= 0)
        {
            var parts = url[(sshIndex + sshMarker.Length)..].TrimEnd('/').Split('/');
            if (parts.Length < 3) return null;
            return new AzureDevOpsRemote(
                $"https://dev.azure.com/{Uri.EscapeDataString(parts[0])}",
                Uri.EscapeDataString(parts[1]),
                Uri.EscapeDataString(StripDotGit(parts[2])));
        }

        var custom = TryParseCustomBase(customBaseUrl);

        // SSH against the custom host: ssh://[user@]{host}[:port]/{collection}/{project}/_git/{repo}
        if (url.StartsWith("ssh://", StringComparison.OrdinalIgnoreCase))
        {
            if (custom is null) return null;
            var authorityEnd = url.IndexOf('/', 6);
            if (authorityEnd < 0) return null;
            var authority = url[6..authorityEnd];
            var at = authority.LastIndexOf('@');
            if (at >= 0) authority = authority[(at + 1)..];
            var colon = authority.LastIndexOf(':');
            if (colon >= 0) authority = authority[..colon];
            if (!authority.Equals(custom.Host, StringComparison.OrdinalIgnoreCase)) return null;
            var sshSegments = Uri.UnescapeDataString(url[(authorityEnd + 1)..])
                .Split('/', StringSplitOptions.RemoveEmptyEntries);
            // The REST API lives on the configured base's scheme/port, not the SSH daemon's.
            return FromGitSegments(custom.GetLeftPart(UriPartial.Authority), sshSegments);
        }

        if (!url.StartsWith("http://", StringComparison.OrdinalIgnoreCase)
            && !url.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }
        if (!Uri.TryCreate(url, UriKind.Absolute, out var uri)) return null;

        // AbsolutePath arrives escaped (%20); unescape first so the per-segment
        // EscapeDataString below does not double-encode it.
        var segments = Uri.UnescapeDataString(uri.AbsolutePath)
            .Split('/', StringSplitOptions.RemoveEmptyEntries);
        if (uri.Host.Equals("dev.azure.com", StringComparison.OrdinalIgnoreCase))
        {
            // HTTPS: https://dev.azure.com/{org}/{project}/_git/{repo}
            if (segments.Length >= 4 && segments[2].Equals("_git", StringComparison.OrdinalIgnoreCase))
            {
                return new AzureDevOpsRemote(
                    $"https://dev.azure.com/{Uri.EscapeDataString(segments[0])}",
                    Uri.EscapeDataString(segments[1]),
                    Uri.EscapeDataString(StripDotGit(segments[3])));
            }
            return null;
        }

        // Legacy HTTPS host: https://{org}.visualstudio.com/{project}/_git/{repo}
        if (uri.Host.EndsWith(".visualstudio.com", StringComparison.OrdinalIgnoreCase)
            && segments.Length >= 3
            && segments[1].Equals("_git", StringComparison.OrdinalIgnoreCase))
        {
            var org = uri.Host[..^".visualstudio.com".Length];
            return new AzureDevOpsRemote(
                $"https://dev.azure.com/{Uri.EscapeDataString(org)}",
                Uri.EscapeDataString(segments[0]),
                Uri.EscapeDataString(StripDotGit(segments[2])));
        }

        // Custom server: https://{host}[/app-path]/{collection}/{project}/_git/{repo}
        if (custom is not null && uri.Host.Equals(custom.Host, StringComparison.OrdinalIgnoreCase))
        {
            return FromGitSegments($"{uri.Scheme}://{uri.Authority}", segments);
        }
        return null;
    }

    /// <summary>Parses the settings' custom server URL; <see langword="null"/> when unset
    /// or not an absolute http(s) URL. The host is the matching key, the authority the
    /// REST base for custom-host SSH remotes.</summary>
    private static Uri? TryParseCustomBase(string? customBaseUrl)
    {
        if (string.IsNullOrWhiteSpace(customBaseUrl)) return null;
        return Uri.TryCreate(customBaseUrl, UriKind.Absolute, out var uri)
               && (uri.Scheme == Uri.UriSchemeHttp || uri.Scheme == Uri.UriSchemeHttps)
            ? uri
            : null;
    }

    /// <summary>
    /// Builds a remote from path segments shaped <c>[{orgPath}/]{project}/_git/{repo}</c>:
    /// the project is the segment right before the <c>_git</c> marker, the repository the
    /// one right after it, and anything before the project forms the API base's path
    /// (organization and/or one or more collection segments). Returns
    /// <see langword="null"/> when the segments do not have that shape.
    /// </summary>
    private static AzureDevOpsRemote? FromGitSegments(string baseWithoutPath, string[] segments)
    {
        for (var i = 1; i < segments.Length - 1; i++)
        {
            if (!segments[i].Equals("_git", StringComparison.OrdinalIgnoreCase)) continue;
            var orgPath = string.Join("/", segments.Take(i - 1).Select(Uri.EscapeDataString));
            return new AzureDevOpsRemote(
                orgPath.Length > 0 ? $"{baseWithoutPath}/{orgPath}" : baseWithoutPath,
                Uri.EscapeDataString(segments[i - 1]),
                Uri.EscapeDataString(StripDotGit(segments[i + 1])));
        }
        return null;
    }

    private static string StripDotGit(string segment)
        => segment.EndsWith(".git", StringComparison.OrdinalIgnoreCase) ? segment[..^4] : segment;
}
