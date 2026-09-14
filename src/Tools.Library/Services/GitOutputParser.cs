using Tools.Library.Entities;

namespace Tools.Library.Services;

/// <summary>
/// The one git CLI output parser that survived the in-process read migration: the
/// stderr line picker for sync failures (local reads now run through libgit2 in
/// <see cref="GitReadService"/> and need no output parsing at all). Pure function —
/// no process spawning, no entity mutation.
/// </summary>
internal static class GitOutputParser
{
    /// <summary>
    /// Picks the actionable line from git's stderr for a sync failure — the first
    /// fatal/error/conflict/rejection line, falling back to the last non-empty line
    /// (transfer chatter like "From origin" would otherwise fill the notification).
    /// </summary>
    public static string? SummarizeSyncError(List<string> lines)
    {
        string? fallback = null;
        foreach (var raw in lines)
        {
            var line = raw.Trim();
            if (line.Length == 0) continue;
            if (line.StartsWith("fatal: ", StringComparison.Ordinal)
                || line.StartsWith("error: ", StringComparison.Ordinal)
                || line.StartsWith("CONFLICT", StringComparison.Ordinal)
                || line.StartsWith("! [", StringComparison.Ordinal))
            {
                return line;
            }
            fallback = line;
        }
        return fallback;
    }
}

/// <summary>What a <see cref="GitBranchRef"/> row in the branch dropdown represents.</summary>
public enum GitBranchKind
{
    /// <summary>A local branch (<c>refs/heads/…</c>); selecting it checks it out.</summary>
    Local,

    /// <summary>A remote-tracking branch (<c>refs/remotes/…</c>, e.g. <c>origin/foo</c>); selecting it checks out the local tracking equivalent.</summary>
    Remote,

    /// <summary>A non-selectable group label (e.g. the "Remote" divider row).</summary>
    Header,
}

/// <summary>
/// One row of the branch dropdown: an immutable value type so list diffing can use
/// plain sequence equality; the computed kind flags drive the row template without
/// converters.
/// </summary>
public sealed record GitBranchRef(string Name, GitBranchKind Kind)
{
    public bool IsRemote => Kind == GitBranchKind.Remote;
    public bool IsLocal => Kind == GitBranchKind.Local;
    public bool IsHeader => Kind == GitBranchKind.Header;
}
