using System.Globalization;
using Tools.Library.Entities;

namespace Tools.Library.Services;

/// <summary>
/// Static parsers for git CLI output: the porcelain v2 status (branch header + change
/// lines), the per-line status shaping the Changes tab's two views need, numstat
/// counts, and the stderr line picker for sync failures. Pure functions — no process
/// spawning, no entity mutation; <see cref="GitStatusService"/> feeds them and applies
/// the results.
/// </summary>
internal static class GitOutputParser
{
    /// <summary>
    /// Parses <c>git status --porcelain=v2 --branch --untracked-files=all</c> output.
    /// Header lines look like <c># branch.head main</c> and <c># branch.ab +2 -1</c>
    /// (the latter only when an upstream is configured); every remaining line is one
    /// change entry (ordinary, renamed, unmerged or untracked). A null/empty output
    /// yields a zeroed snapshot.
    /// </summary>
    public static GitStatusSnapshot ParsePorcelain(string? output)
    {
        string? branch = null;
        var modified = 0;
        var ahead = 0;
        var behind = 0;

        if (!string.IsNullOrEmpty(output))
        {
            ForEachLine(output, line =>
            {
                if (line.StartsWith("# branch.head ", StringComparison.Ordinal))
                {
                    branch = line["# branch.head ".Length..].Trim().ToString();
                }
                else if (line.StartsWith("# branch.ab ", StringComparison.Ordinal))
                {
                    var values = line["# branch.ab ".Length..].Trim();
                    Span<Range> parts = stackalloc Range[4];
                    var count = values.Split(parts, ' ', StringSplitOptions.RemoveEmptyEntries);
                    for (var i = 0; i < count; i++)
                    {
                        var part = values[parts[i]];
                        if (part.StartsWith('+'))
                            int.TryParse(part[1..], out ahead);
                        else if (part.StartsWith('-'))
                            int.TryParse(part[1..], out behind);
                    }
                }
                else if (!line.StartsWith('#'))
                {
                    modified++;
                }
            });
        }

        return new GitStatusSnapshot(branch, modified, ahead, behind);
    }

    /// <summary>The parsed result of one repo's git status probe.</summary>
    public sealed record GitStatusSnapshot(string? BranchName, int ModifiedCount, int AheadCount, int BehindCount);

    /// <summary>
    /// Walks the lines of a multi-line CLI output as spans — the allocation-free
    /// alternative to <c>Split('\n')</c>, which duplicates the whole output as
    /// per-line strings on every pass (a status output can hold thousands of
    /// untracked entries). Mirrors <c>StringSplitOptions.RemoveEmptyEntries</c>:
    /// empty and whitespace-only lines are skipped, and each line is handed over
    /// \r-trimmed. The span is only valid for the duration of the call.
    /// </summary>
    public delegate void LineHandler(ReadOnlySpan<char> line);

    public static void ForEachLine(string output, LineHandler handle)
    {
        var span = output.AsSpan();
        while (!span.IsEmpty)
        {
            var eol = span.IndexOf('\n');
            var line = eol < 0 ? span : span[..eol];
            span = eol < 0 ? default : span[(eol + 1)..];
            line = line.TrimEnd('\r');
            if (line.IsWhiteSpace()) continue;
            handle(line);
        }
    }

    /// <summary>
    /// Parses one porcelain v2 change line into the two views the Changes tab shows.
    /// Ordinary/renamed entries (<c>1</c>/<c>2</c>): the XY pair right after the kind —
    /// X is the index (staged) status, Y the worktree (unstaged) status, '.' meaning
    /// unmodified on that side; the path is the LAST space-separated token (a rename's
    /// original path precedes it). The file list keeps the whole XY pair verbatim and
    /// only non-empty paths; the split view emits one entry per non-'.' side. Unmerged
    /// entries (<c>u</c>) surface as a "U" worktree entry — there is no clean index
    /// side to stage until the conflict is resolved — while the file list reads the XY
    /// pair like any ordinary line. Untracked entries (<c>?</c>): the whole remainder
    /// IS the path (it may contain spaces), worktree side only.
    /// </summary>
    public static void ParseStatusLine(
        ReadOnlySpan<char> line,
        List<GitChangedFile> statusFiles,
        List<GitChangedFile> staged,
        List<GitChangedFile> unstaged)
    {
        var separator = line.IndexOf(' ');
        if (separator <= 0) return;

        var kind = line[..separator];
        var rest = line[(separator + 1)..];

        if (kind.SequenceEqual("?"))
        {
            if (!rest.IsEmpty) statusFiles.Add(new GitChangedFile(rest.ToString(), "?"));
            unstaged.Add(new GitChangedFile(rest.ToString(), "?"));
            return;
        }

        if (kind.SequenceEqual("u"))
        {
            unstaged.Add(new GitChangedFile(LastToken(rest).ToString(), "U"));

            var unmergedCodeEnd = rest.IndexOf(' ');
            if (unmergedCodeEnd <= 0) return;
            var unmergedPath = LastToken(rest[(unmergedCodeEnd + 1)..]);
            if (!unmergedPath.IsEmpty)
            {
                statusFiles.Add(new GitChangedFile(unmergedPath.ToString(), rest[..unmergedCodeEnd].ToString()));
            }
            return;
        }

        if (rest.Length < 3) return;
        var indexCode = rest[0];
        var worktreeCode = rest[1];
        var path = LastToken(rest[3..]);

        if (indexCode is not ('.' or ' '))
        {
            staged.Add(new GitChangedFile(path.ToString(), indexCode.ToString()));
        }
        if (worktreeCode is not ('.' or ' '))
        {
            unstaged.Add(new GitChangedFile(path.ToString(), worktreeCode.ToString()));
        }
        if (!path.IsEmpty)
        {
            // (Path, StatusCode) — the flat file list shows the path and the verbatim
            // XY pair as its status.
            statusFiles.Add(new GitChangedFile(path.ToString(), rest[..2].ToString()));
        }
    }

    /// <summary>The last space-separated token of a porcelain v2 change line.</summary>
    private static ReadOnlySpan<char> LastToken(ReadOnlySpan<char> fields)
    {
        var lastSpace = fields.LastIndexOf(' ');
        return lastSpace >= 0 ? fields[(lastSpace + 1)..] : fields;
    }

    /// <summary>Parses <c>git diff --numstat</c> output into per-path add/delete counts.</summary>
    public static Dictionary<string, (int? Additions, int? Deletions)> ParseNumstat(string? output)
    {
        var counts = new Dictionary<string, (int?, int?)>(StringComparer.Ordinal);
        if (string.IsNullOrEmpty(output)) return counts;

        foreach (var rawLine in output.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var line = rawLine.TrimEnd('\r');
            // "additions\tdeletions\tpath" — the path may contain spaces (and renames
            // render as "old => new" or "{prefix old => suffix new}"), so split exactly
            // two tab-separated counts off the front.
            var firstTab = line.IndexOf('\t');
            if (firstTab <= 0) continue;
            var secondTab = line.IndexOf('\t', firstTab + 1);
            if (secondTab <= 0) continue;

            var path = line[(secondTab + 1)..];
            var arrow = path.LastIndexOf(" => ", StringComparison.Ordinal);
            if (arrow >= 0)
            {
                path = path[(arrow + 4)..];
            }

            int? additions = int.TryParse(line[..firstTab], out var a) ? a : null;
            int? deletions = int.TryParse(line[(firstTab + 1)..secondTab], out var d) ? d : null;
            counts[path] = (additions, deletions);
        }

        return counts;
    }

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
