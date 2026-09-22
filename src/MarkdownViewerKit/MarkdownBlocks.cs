using System.Text.RegularExpressions;

namespace MarkdownViewerKit;

/// <summary>Kind of a top-level markdown block (the live editor labels, alignment
/// only needs the spans).</summary>
public enum MarkdownBlockKind
{
    Heading,
    SetextHeading,
    Paragraph,
    FencedCode,
    Blockquote,
    List,
    Table,
    ThematicBreak,
}

/// <summary>A top-level markdown block: the [Start, End) char span of its source
/// lines (newlines excluded) and the inclusive line range.</summary>
public readonly record struct MarkdownBlock(
    int Start,
    int End,
    int StartLine,
    int EndLine,
    MarkdownBlockKind Kind);

/// <summary>
/// Splits markdown source into top-level blocks, mirroring how the Markdown.Avalonia
/// engine groups rendered output (verified empirically against the Tight fork): loose
/// list items stay one list, a different marker starts a new list, blockquotes have no
/// lazy continuation, frontmatter is not special (the opening dashes are a thematic
/// break) and a setext underline heads only the paragraph's last line. Rendered
/// top-level children align with these blocks 1:1 for notes-shaped documents;
/// consumers must compare counts before trusting the alignment.
/// </summary>
public static partial class MarkdownBlocks
{
    public static IReadOnlyList<MarkdownBlock> Parse(string text)
    {
        var blocks = new List<MarkdownBlock>();
        if (string.IsNullOrEmpty(text))
        {
            return blocks;
        }

        var starts = new List<int> { 0 };
        for (var i = 0; i < text.Length; i++)
        {
            if (text[i] == '\n')
            {
                starts.Add(i + 1);
            }
        }

        var lineCount = starts.Count;
        int LineEnd(int line) => line + 1 < lineCount ? starts[line + 1] - 1 : text.Length;
        string LineText(int line) => text[starts[line]..LineEnd(line)].TrimEnd('\r');
        bool TableInterrupts(int line)
            => line + 1 < lineCount
                && LineText(line).Contains('|')
                && TableSeparator().IsMatch(LineText(line + 1).Trim());

        void Emit(int startLine, int endLine, MarkdownBlockKind kind)
            => blocks.Add(new MarkdownBlock(starts[startLine], LineEnd(endLine), startLine, endLine, kind));

        var line = 0;
        while (line < lineCount)
        {
            var raw = LineText(line);
            var trimmed = raw.TrimStart();
            if (trimmed.Length == 0)
            {
                line++;
                continue;
            }

            if (FenceMatch(trimmed) is { } fence)
            {
                var end = ScanFenceEnd(lineCount, LineText, line, fence.Char, fence.Len);
                Emit(line, end, MarkdownBlockKind.FencedCode);
                line = end + 1;
                continue;
            }

            if (AtxHeading().IsMatch(trimmed))
            {
                Emit(line, line, MarkdownBlockKind.Heading);
                line++;
                continue;
            }

            if (BlockquoteStart().IsMatch(raw))
            {
                var end = line;
                while (end + 1 < lineCount && BlockquoteStart().IsMatch(LineText(end + 1)))
                {
                    end++;
                }

                Emit(line, end, MarkdownBlockKind.Blockquote);
                line = end + 1;
                continue;
            }

            if (ThematicBreak().IsMatch(trimmed))
            {
                Emit(line, line, MarkdownBlockKind.ThematicBreak);
                line++;
                continue;
            }

            if (ListItemStart().IsMatch(trimmed))
            {
                var end = ScanListEnd(lineCount, LineText, line, ListMarker(trimmed));
                Emit(line, end, MarkdownBlockKind.List);
                line = end + 1;
                continue;
            }

            if (line + 1 < lineCount
                && raw.Contains('|')
                && TableSeparator().IsMatch(LineText(line + 1).Trim()))
            {
                var end = line + 1;
                while (end + 1 < lineCount && LineText(end + 1).Contains('|') && LineText(end + 1).Trim().Length > 0)
                {
                    end++;
                }

                Emit(line, end, MarkdownBlockKind.Table);
                line = end + 1;
                continue;
            }

            // Paragraph run: ends at a blank line or any block start; a dash/equals
            // line is a setext underline. The engine heads only the run's LAST line —
            // earlier lines stay a paragraph of their own.
            var endLine = line;
            while (endLine + 1 < lineCount)
            {
                var nextRaw = LineText(endLine + 1);
                var next = nextRaw.TrimStart();
                if (next.Length == 0
                    || FenceMatch(next) is not null
                    || AtxHeading().IsMatch(next)
                    || BlockquoteStart().IsMatch(nextRaw)
                    || SetextDash().IsMatch(next)
                    || SetextEquals().IsMatch(next)
                    || ThematicBreak().IsMatch(next)
                    || ListItemStart().IsMatch(next)
                    || TableInterrupts(endLine + 1))
                {
                    break;
                }

                endLine++;
            }

            var setext = endLine + 1 < lineCount
                && (SetextDash().IsMatch(LineText(endLine + 1).TrimStart())
                    || SetextEquals().IsMatch(LineText(endLine + 1).TrimStart()));
            if (setext)
            {
                if (endLine > line)
                {
                    Emit(line, endLine - 1, MarkdownBlockKind.Paragraph);
                    Emit(endLine, endLine + 1, MarkdownBlockKind.SetextHeading);
                }
                else
                {
                    Emit(line, endLine + 1, MarkdownBlockKind.SetextHeading);
                }

                line = endLine + 2;
            }
            else
            {
                Emit(line, endLine, MarkdownBlockKind.Paragraph);
                line = endLine + 1;
            }
        }

        return blocks;
    }

    private static (char Char, int Len)? FenceMatch(string trimmed)
    {
        if (trimmed.Length < 3)
        {
            return null;
        }

        var c = trimmed[0];
        if (c is not '`' and not '~')
        {
            return null;
        }

        var len = 0;
        while (len < trimmed.Length && trimmed[len] == c)
        {
            len++;
        }

        return len >= 3 ? (c, len) : null;
    }

    private static int ScanFenceEnd(int lineCount, Func<int, string> lineText, int open, char fenceChar, int fenceLen)
    {
        for (var line = open + 1; line < lineCount; line++)
        {
            var t = lineText(line).Trim();
            if (t.Length >= fenceLen && t.All(c => c == fenceChar))
            {
                return line;
            }
        }

        return lineCount - 1;
    }

    /// <summary>The list "flavor" the engine keeps one grid for: the bullet char, or
    /// the ordered delimiter. A different marker starts a separate rendered list.</summary>
    private static string ListMarker(string trimmed)
    {
        if (trimmed[0] is '-' or '*' or '+')
        {
            return trimmed[..1];
        }

        var close = trimmed.IndexOf(')');
        return close > 0 && trimmed[close - 1] != '.' ? ")" : ".";
    }

    private static int ScanListEnd(int lineCount, Func<int, string> lineText, int start, string marker)
    {
        var line = start;
        while (line < lineCount)
        {
            var raw = lineText(line);
            var trimmed = raw.TrimStart();
            if (trimmed.Length == 0)
            {
                // Blank lines keep the list together only when another item follows
                // (loose list — the engine still renders one grid).
                var next = line;
                while (next < lineCount && lineText(next).TrimStart().Length == 0)
                {
                    next++;
                }

                if (next < lineCount
                    && ListItemStart().IsMatch(lineText(next).TrimStart())
                    && ListMarker(lineText(next).TrimStart()) == marker)
                {
                    line = next;
                    continue;
                }

                break;
            }

            if (ListItemStart().IsMatch(trimmed) && ListMarker(trimmed) == marker || raw[0] is ' ' or '\t')
            {
                line++;
                continue;
            }

            break;
        }

        return line - 1;
    }

    [GeneratedRegex(@"^#{1,6}([ \t].*)?$")]
    private static partial Regex AtxHeading();

    [GeneratedRegex(@"^[ \t]{0,3}>")]
    private static partial Regex BlockquoteStart();

    [GeneratedRegex(@"^([-*_])[ \t]*(\1[ \t]*){2,}$")]
    private static partial Regex ThematicBreak();

    [GeneratedRegex(@"^-+[ \t]*$")]
    private static partial Regex SetextDash();

    [GeneratedRegex(@"^=+[ \t]*$")]
    private static partial Regex SetextEquals();

    [GeneratedRegex(@"^([-*+]([ \t]+|$))|(\d{1,9}[.)]([ \t]+|$))")]
    private static partial Regex ListItemStart();

    [GeneratedRegex(@"^\|?[ \t]*:?-+:?[ \t]*(\|[ \t]*:?-+:?[ \t]*)*\|?[ \t]*$")]
    private static partial Regex TableSeparator();
}
