using System.Text.RegularExpressions;

namespace Tools.Helpers;

/// <summary>Result of a text edit: the new text plus where the editor selection
/// should land afterwards.</summary>
public readonly record struct MarkdownEditResult(string Text, int SelectionStart, int SelectionLength);

/// <summary>
/// Pure string operations behind the notes editor's formatting toolbar and keys
/// (wrap markers, line prefixes, list continuation, indent). No UI state — the
/// code-behind supplies the text/selection and applies the result to the TextBox.
/// </summary>
public static partial class MarkdownEditing
{
    public const string Bold = "bold";
    public const string Italic = "italic";
    public const string Strikethrough = "strike";
    public const string InlineCode = "code";
    public const string CodeBlock = "codeblock";
    public const string Link = "link";
    public const string Heading1 = "h1";
    public const string Heading2 = "h2";
    public const string Heading3 = "h3";
    public const string BulletList = "bullet";
    public const string OrderedList = "ordered";
    public const string TaskList = "task";
    public const string Blockquote = "quote";
    public const string Rule = "hr";

    private const string PlaceholderText = "text";

    [GeneratedRegex(@"^(\s{0,3})(#{1,6})(\s+)(.*)$")]
    private static partial Regex HeadingPrefix();

    [GeneratedRegex(@"^(\s*)[-*+]\s+\[([ xX])\]\s*(.*)$")]
    private static partial Regex TaskPrefix();

    // [ \t] rather than \s: \s crosses newlines, so a task preceded by a blank line
    // matched from that line's start and its computed line index came back one low
    // (the toggle then hit the blank line and silently no-oped).
    [GeneratedRegex(@"^([ \t]*)[-*+][ \t]+\[([ xX])\][ \t]*(.*)$", RegexOptions.Multiline)]
    private static partial Regex TaskBoxLine();

    [GeneratedRegex(@"^(\s*)([-*+])\s+(.*)$")]
    private static partial Regex BulletPrefix();

    [GeneratedRegex(@"^(\s*)(\d+)([.)])\s+(.*)$")]
    private static partial Regex OrderedPrefix();

    [GeneratedRegex(@"^(\s*)>\s?(.*)$")]
    private static partial Regex QuotePrefix();

    public static MarkdownEditResult Apply(string format, string text, int start, int length)
    {
        (start, length) = Clamp(text, start, length);
        return format switch
        {
            Bold => WrapSelection(text, start, length, "**"),
            Italic => WrapSelection(text, start, length, "*"),
            Strikethrough => WrapSelection(text, start, length, "~~"),
            InlineCode => WrapSelection(text, start, length, "`"),
            CodeBlock => WrapCodeBlock(text, start, length),
            Link => InsertLink(text, start, length),
            Heading1 => ToggleLinePrefix(text, start, length, ApplyHeading(1)),
            Heading2 => ToggleLinePrefix(text, start, length, ApplyHeading(2)),
            Heading3 => ToggleLinePrefix(text, start, length, ApplyHeading(3)),
            BulletList => ToggleLinePrefix(text, start, length, ApplyBullet),
            OrderedList => ToggleLinePrefix(text, start, length, null, ApplyOrdered),
            TaskList => ToggleLinePrefix(text, start, length, ApplyTask),
            Blockquote => ToggleLinePrefix(text, start, length, ApplyQuote),
            Rule => InsertRule(text, start),
            _ => new MarkdownEditResult(text, start, length),
        };
    }

    private static (int Start, int Length) Clamp(string text, int start, int length)
    {
        start = Math.Clamp(start, 0, text.Length);
        length = Math.Clamp(length, 0, text.Length - start);
        return (start, length);
    }

    /// <summary>Wraps the selection (or a placeholder word when nothing is selected) in
    /// the marker; a selection already wrapped exactly once unwraps instead.</summary>
    private static MarkdownEditResult WrapSelection(string text, int start, int length, string marker)
    {
        if (length >= marker.Length * 2
            && start >= marker.Length
            && start + length + marker.Length <= text.Length
            && text.Substring(start - marker.Length, marker.Length) == marker
            && text.Substring(start + length, marker.Length) == marker)
        {
            var unwrapped = text.Remove(start + length, marker.Length).Remove(start - marker.Length, marker.Length);
            return new MarkdownEditResult(unwrapped, start - marker.Length, length);
        }

        var inner = length > 0 ? text.Substring(start, length) : PlaceholderText;
        var newText = text.Remove(start, length).Insert(start, marker + inner + marker);
        return new MarkdownEditResult(newText, start + marker.Length, inner.Length);
    }

    private static MarkdownEditResult WrapCodeBlock(string text, int start, int length)
    {
        if (length == 0)
        {
            const string block = "```\n" + PlaceholderText + "\n```";
            return new MarkdownEditResult(text.Insert(start, block), start + 4, PlaceholderText.Length);
        }

        var inner = text.Substring(start, length);
        var newText = text.Remove(start, length).Insert(start, "```\n" + inner + "\n```");
        return new MarkdownEditResult(newText, start + 4, length);
    }

    private static MarkdownEditResult InsertLink(string text, int start, int length)
    {
        var label = length > 0 ? text.Substring(start, length) : PlaceholderText;
        var newText = text.Remove(start, length).Insert(start, $"[{label}](url)");
        return new MarkdownEditResult(newText, start + label.Length + 3, 3);
    }

    private static MarkdownEditResult InsertRule(string text, int start)
    {
        var before = start > 0 ? text[..start] : string.Empty;
        var prefix = before.Length == 0 || before.EndsWith("\n\n") ? string.Empty
            : before.EndsWith('\n') ? "\n"
            : "\n\n";
        var insertion = prefix + "---\n\n";
        return new MarkdownEditResult(text.Insert(start, insertion), start + insertion.Length, 0);
    }

    /// <summary>Applies (or toggles off) a per-line transform across every selected
    /// line; the selection afterwards covers the whole touched region.</summary>
    private static MarkdownEditResult ToggleLinePrefix(
        string text, int start, int length, Func<string, string?>? prefixApplier, Func<string, int, string?>? orderedApplier = null)
    {
        var lineStart = text.LastIndexOf('\n', Math.Max(start - 1, 0));
        lineStart = lineStart < 0 ? 0 : lineStart + 1;
        var regionEnd = text.IndexOf('\n', start + length);
        regionEnd = regionEnd < 0 ? text.Length : regionEnd;

        var lines = text[lineStart..regionEnd].Split('\n');
        var orderedNumber = 1;
        for (var i = 0; i < lines.Length; i++)
        {
            var transformed = orderedApplier is not null ? orderedApplier(lines[i], orderedNumber) : prefixApplier?.Invoke(lines[i]);
            if (transformed is not null)
            {
                lines[i] = transformed;
                orderedNumber++;
            }
        }

        var newRegion = string.Join('\n', lines);
        var newText = text[..lineStart] + newRegion + text[regionEnd..];
        return new MarkdownEditResult(newText, lineStart, newRegion.Length);
    }

    private static Func<string, string?> ApplyHeading(int level)
        => line =>
        {
            if (string.IsNullOrWhiteSpace(line))
            {
                return new string('#', level) + " ";
            }

            var match = HeadingPrefix().Match(line);
            if (match.Success && match.Groups[2].Length == level)
            {
                return match.Groups[1].Value + match.Groups[4].Value;
            }

            var body = match.Success ? match.Groups[4].Value : line.TrimStart();
            return line[..(line.Length - line.TrimStart().Length)] + new string('#', level) + " " + body;
        };

    private static string? ApplyBullet(string line)
    {
        if (line.Trim().Length == 0)
        {
            return "- ";
        }

        var match = BulletPrefix().Match(line);
        return match.Success ? match.Groups[1].Value + match.Groups[3].Value : PrefixLine(line, "- ");
    }

    private static string? ApplyTask(string line)
    {
        var bullet = BulletPrefix().Match(line);
        if (TaskPrefix().Match(line) is { Success: true } task)
        {
            return task.Groups[1].Value + task.Groups[3].Value;
        }

        if (bullet.Success)
        {
            return $"{bullet.Groups[1].Value}{bullet.Groups[2].Value} [ ] {bullet.Groups[3].Value}";
        }

        return line.Trim().Length == 0 ? "- [ ] " : PrefixLine(line, "- [ ] ");
    }

    private static string? ApplyQuote(string line)
    {
        var match = QuotePrefix().Match(line);
        if (match.Success)
        {
            return match.Groups[1].Value + match.Groups[2].Value;
        }

        return line.Trim().Length == 0 ? "> " : PrefixLine(line, "> ");
    }

    private static string? ApplyOrdered(string line, int number)
    {
        var match = OrderedPrefix().Match(line);
        if (match.Success)
        {
            return match.Groups[1].Value + match.Groups[4].Value;
        }

        return line.Trim().Length == 0 ? null : PrefixLine(line, $"{number}. ");
    }

    /// <summary>Keeps the line's existing indentation when adding a prefix.</summary>
    private static string PrefixLine(string line, string prefix)
    {
        var trimmed = line.TrimStart();
        return line[..(line.Length - trimmed.Length)] + prefix + trimmed;
    }

    /// <summary>A task item found in the note: its source line and checked state.</summary>
    public readonly record struct PreviewTask(int Line, bool IsChecked);

    /// <summary>Preview-source transform: each task item's checkbox becomes an image
    /// anchor rendered as a live control from the viewer's CascadeResources (key
    /// mtask-&lt;line&gt;); line numbering is preserved 1:1 with the source. Done items
    /// render struck through.</summary>
    public static (string Preview, IReadOnlyList<PreviewTask> Tasks) BuildPreview(string text)
    {
        if (string.IsNullOrEmpty(text))
        {
            return (text, Array.Empty<PreviewTask>());
        }

        var tasks = new List<PreviewTask>();
        var line = 0;
        var scanned = 0;
        var preview = TaskBoxLine().Replace(text, m =>
        {
            line += CountNewlines(text, scanned, m.Index);
            scanned = m.Index;
            var done = m.Groups[2].Value != " ";
            tasks.Add(new PreviewTask(line, done));
            return $"{m.Groups[1].Value}- ![](mtask-{line}) {(done ? $"~~{m.Groups[3].Value}~~" : m.Groups[3].Value)}";
        });
        return (preview, tasks);
    }

    /// <summary>Flips `[ ]`/`[x]` on the given source line; null when that line is not
    /// a task item (e.g. the text changed since the preview rendered).</summary>
    public static string? ToggleTask(string text, int line)
    {
        var start = 0;
        for (var i = 0; i < line; i++)
        {
            start = text.IndexOf('\n', start);
            if (start < 0)
            {
                return null;
            }

            start++;
        }

        var end = text.IndexOf('\n', start);
        end = end < 0 ? text.Length : end;
        var lineText = text[start..end];
        if (TaskPrefix().Match(lineText) is not { Success: true } match)
        {
            return null;
        }

        var mark = match.Groups[2].Value == " " ? "x" : " ";
        var prefix = lineText[..(match.Groups[2].Index - 1)];
        var updated = $"{prefix}[{mark}] {match.Groups[3].Value}";
        return text[..start] + updated + text[end..];
    }

    private static int CountNewlines(string text, int from, int to)
    {
        var count = 0;
        for (var i = from; i < to; i++)
        {
            if (text[i] == '\n')
            {
                count++;
            }
        }

        return count;
    }

    /// <summary>Enter-key list continuation: an empty marker line exits the list, a
    /// populated one starts the next entry; null when the caret is not in a list.</summary>
    public static (string NewText, int NewCaret)? ContinueList(string text, int caret)
    {
        caret = Math.Clamp(caret, 0, text.Length);
        var lineStart = text.LastIndexOf('\n', Math.Max(caret - 1, 0));
        lineStart = lineStart < 0 ? 0 : lineStart + 1;
        var beforeCaret = text[lineStart..caret];

        if (TaskPrefix().Match(beforeCaret) is { Success: true } task && task.Groups[3].Value.Length == 0)
        {
            return ExitList(text, caret, lineStart);
        }

        if (BulletPrefix().Match(beforeCaret) is { Success: true } bullet && bullet.Groups[3].Value.Length == 0)
        {
            return ExitList(text, caret, lineStart);
        }

        var indent = beforeCaret[..(beforeCaret.Length - beforeCaret.TrimStart().Length)];
        string continuation;
        if (TaskPrefix().Match(beforeCaret) is { Success: true } t)
        {
            continuation = $"{t.Groups[1].Value}- [ ] ";
        }
        else if (BulletPrefix().Match(beforeCaret) is { Success: true } b)
        {
            continuation = $"{b.Groups[1].Value}{b.Groups[2].Value} ";
        }
        else if (OrderedPrefix().Match(beforeCaret) is { Success: true } o)
        {
            continuation = $"{o.Groups[1].Value}{int.Parse(o.Groups[2].Value) + 1}{o.Groups[3].Value} ";
        }
        else
        {
            return null;
        }

        return (text.Insert(caret, "\n" + continuation), caret + 1 + continuation.Length);
    }

    /// <summary>Leaving a list from a marker-only line removes the marker and the line
    /// break before it, so the caret joins the previous line's end.</summary>
    private static (string NewText, int NewCaret) ExitList(string text, int caret, int lineStart)
    {
        var removeStart = lineStart > 0 ? lineStart - 1 : lineStart;
        return (text.Remove(removeStart, caret - removeStart), removeStart);
    }

    /// <summary>Tab/Shift+Tab: indent or outdent the selected lines by two spaces;
    /// a caret-only Tab inserts two spaces.</summary>
    public static MarkdownEditResult Indent(string text, int start, int length, bool outdent)
    {
        (start, length) = Clamp(text, start, length);
        if (length == 0 && !outdent)
        {
            return new MarkdownEditResult(text.Insert(start, "  "), start + 2, 0);
        }

        var lineStart = text.LastIndexOf('\n', Math.Max(start - 1, 0));
        lineStart = lineStart < 0 ? 0 : lineStart + 1;
        var regionEnd = text.IndexOf('\n', start + length);
        regionEnd = regionEnd < 0 ? text.Length : regionEnd;

        var lines = text[lineStart..regionEnd].Split('\n');
        for (var i = 0; i < lines.Length; i++)
        {
            if (outdent)
            {
                var removed = 0;
                while (removed < 2 && removed < lines[i].Length && lines[i][removed] == ' ')
                {
                    removed++;
                }

                lines[i] = lines[i][removed..];
            }
            else if (lines[i].Length > 0)
            {
                lines[i] = "  " + lines[i];
            }
        }

        var newRegion = string.Join('\n', lines);
        return new MarkdownEditResult(text[..lineStart] + newRegion + text[regionEnd..], lineStart, newRegion.Length);
    }
}
