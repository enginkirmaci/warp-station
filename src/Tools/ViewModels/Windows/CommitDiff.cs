using System.Text.RegularExpressions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// The kinds of line a GitHub-style diff body renders. <see cref="DiffLineKind.Hunk"/>
/// rows carry the @@ range header, <see cref="DiffLineKind.Meta"/> rows are the
/// patch's file-header lines (diff --git / index / --- / +++) which the viewer skips.
/// </summary>
public enum DiffLineKind
{
    Context,
    Add,
    Delete,
    Hunk,
    NoNewline,
}

/// <summary>
/// One rendered row of a file's diff: the kind-driven coloring plus the two gutter
/// line numbers (empty where GitHub leaves them blank) and the code text with its
/// sign already stripped — the gutter renders the sign instead.
/// </summary>
public sealed record DiffLineRow(DiffLineKind Kind, string OldNumber, string NewNumber, string Text)
{
    public bool IsAdd => Kind == DiffLineKind.Add;

    public bool IsDelete => Kind == DiffLineKind.Delete;

    public bool IsHunk => Kind == DiffLineKind.Hunk;

    public bool IsNoNewline => Kind == DiffLineKind.NoNewline;

    /// <summary>The gutter sign: + for additions, − for deletions, blank otherwise.</summary>
    public string Sign => Kind switch
    {
        DiffLineKind.Add => "+",
        DiffLineKind.Delete => "-",
        _ => string.Empty,
    };
}

/// <summary>
/// Parses one file's unified-diff text (the <c>git show &lt;hash&gt; -- &lt;path&gt;</c>
/// patch) into the rows the diff viewer renders: hunk headers keep their ranges,
/// content rows get GitHub-style old/new line numbers, and the file-header meta lines
/// before the first hunk are dropped (the file row above the patch already names it).
/// </summary>
public static partial class CommitDiffParser
{
    [GeneratedRegex(@"^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@", RegexOptions.Compiled)]
    private static partial Regex HunkRange();

    public static IReadOnlyList<DiffLineRow> Parse(string? patch)
    {
        if (string.IsNullOrWhiteSpace(patch)) return Array.Empty<DiffLineRow>();

        var rows = new List<DiffLineRow>();
        var oldNo = 0;
        var newNo = 0;
        var inHunk = false;

        foreach (var raw in patch.Split('\n'))
        {
            var line = raw.TrimEnd('\r');
            if (line.Length == 0)
            {
                // The trailing split artifact, or a blank context line inside a hunk.
                if (inHunk) rows.Add(new DiffLineRow(DiffLineKind.Context, oldNo.ToString(), newNo.ToString(), string.Empty));
                continue;
            }

            if (line.StartsWith("@@", StringComparison.Ordinal))
            {
                var match = HunkRange().Match(line);
                oldNo = match.Success ? int.Parse(match.Groups[1].Value) : 0;
                newNo = match.Success ? int.Parse(match.Groups[2].Value) : 0;
                rows.Add(new DiffLineRow(DiffLineKind.Hunk, string.Empty, string.Empty, line));
                inHunk = true;
                continue;
            }

            if (!inHunk) continue;

            switch (line[0])
            {
                case '+':
                    rows.Add(new DiffLineRow(DiffLineKind.Add, string.Empty, (newNo++).ToString(), line[1..]));
                    break;
                case '-':
                    rows.Add(new DiffLineRow(DiffLineKind.Delete, (oldNo++).ToString(), string.Empty, line[1..]));
                    break;
                case ' ':
                    rows.Add(new DiffLineRow(DiffLineKind.Context, (oldNo++).ToString(), (newNo++).ToString(), line[1..]));
                    break;
                case '\\':
                    // "\ No newline at end of file" — rendered as its own muted row.
                    rows.Add(new DiffLineRow(DiffLineKind.NoNewline, string.Empty, string.Empty, line[1..]));
                    break;
                default:
                    // Unknown marker inside a hunk: render it as context, numbers frozen.
                    rows.Add(new DiffLineRow(DiffLineKind.Context, oldNo.ToString(), newNo.ToString(), line));
                    break;
            }
        }

        return rows;
    }
}
