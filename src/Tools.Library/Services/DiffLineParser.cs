namespace Tools.Library.Services;

/// <summary>One row of a unified diff, typed by its role so the colored diff viewer
/// can pick a DataTemplate per shape (derived kinds are top-level types — XAML
/// <c>x:DataType</c> cannot reference nested ones).</summary>
public abstract record DiffLine(string Text);

/// <summary>An added line (<c>+</c> prefix) — green.</summary>
public sealed record DiffAddLine(string Text) : DiffLine(Text);

/// <summary>A removed line (<c>-</c> prefix) — red.</summary>
public sealed record DiffRemoveLine(string Text) : DiffLine(Text);

/// <summary>A hunk header (<c>@@ … @@</c>) — accent.</summary>
public sealed record DiffHunkLine(string Text) : DiffLine(Text);

/// <summary>A metadata row (diff --git / index / --- / +++ / renames) — muted.</summary>
public sealed record DiffMetaLine(string Text) : DiffLine(Text);

/// <summary>An unchanged context line — plain theme text.</summary>
public sealed record DiffContextLine(string Text) : DiffLine(Text);

/// <summary>
/// Splits a unified diff's text into typed <see cref="DiffLine"/> rows. The line text
/// is kept verbatim, markers included, so the mono column stays aligned with git's
/// own output.
/// </summary>
public static class DiffLineParser
{
    public static IReadOnlyList<DiffLine> Parse(string? patch)
    {
        if (string.IsNullOrWhiteSpace(patch))
        {
            return Array.Empty<DiffLine>();
        }

        var raw = patch.Replace("\r\n", "\n").Split('\n');
        if (raw[^1].Length == 0)
        {
            // The split's trailing artifact after the final newline.
            Array.Resize(ref raw, raw.Length - 1);
        }

        var lines = new List<DiffLine>(raw.Length);
        foreach (var line in raw)
        {
            lines.Add(line switch
            {
                var t when t.StartsWith("diff --git")
                    || t.StartsWith("index ")
                    || t.StartsWith("old mode ")
                    || t.StartsWith("new mode ")
                    || t.StartsWith("new file mode")
                    || t.StartsWith("deleted file mode")
                    || t.StartsWith("similarity index ")
                    || t.StartsWith("rename from ")
                    || t.StartsWith("rename to ")
                    || t.StartsWith("Binary files ")
                    || t.StartsWith("GIT binary patch")
                    || t.StartsWith("--- ")
                    || t.StartsWith("+++ ")
                    || t.StartsWith('\\')
                    || t.EndsWith("(patch truncated)") => new DiffMetaLine(t),
                var t when t.StartsWith("@@") => new DiffHunkLine(t),
                var t when t.StartsWith('+') => new DiffAddLine(t),
                var t when t.StartsWith('-') => new DiffRemoveLine(t),
                _ => new DiffContextLine(line),
            });
        }

        return lines;
    }
}
