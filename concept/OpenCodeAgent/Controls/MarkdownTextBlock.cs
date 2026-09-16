using System.Text;
using System.Text.RegularExpressions;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Documents;
using Avalonia.Layout;
using Avalonia.Media;

namespace OpenCodeAgent.Controls;

/// <summary>
/// Lightweight markdown renderer for chat answers: headings, bold/italic,
/// inline code, fenced code blocks, lists, links. Rebuilds its children on
/// every Text change so streaming updates just work.
/// </summary>
public class MarkdownTextBlock : StackPanel
{
    private static readonly FontFamily CodeFont = FontFamily.Parse("DejaVu Sans Mono, Consolas, monospace");
    private static readonly IBrush CodeBackground = new SolidColorBrush(Color.Parse("#FF262B36"));
    private static readonly IBrush CodeForeground = new SolidColorBrush(Color.Parse("#FFE8D9BC"));
    private static readonly IBrush BlockBackground = new SolidColorBrush(Color.Parse("#FF1B1F28"));
    private static readonly IBrush LinkForeground = new SolidColorBrush(Color.Parse("#FF6EC0FF"));

    private static readonly Regex LinkRegex = new(@"\[([^\]]+)\]\(([^)\s]+)\)", RegexOptions.Compiled);
    private static readonly Regex OrderedRegex = new(@"^\d+[.)]\s+(.*)$", RegexOptions.Compiled);

    public static readonly StyledProperty<string> TextProperty =
        AvaloniaProperty.Register<MarkdownTextBlock, string>(nameof(Text), "");

    public string Text
    {
        get => GetValue(TextProperty);
        set => SetValue(TextProperty, value);
    }

    public MarkdownTextBlock()
    {
        Spacing = 6;
        TextProperty.Changed.AddClassHandler<MarkdownTextBlock>((c, _) => c.Rebuild());
    }

    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        Rebuild();
    }

    private void Rebuild()
    {
        Children.Clear();
        foreach (var block in Parse(Text))
            Children.Add(Build(block));
    }

    private static Control Build(Block block) => block switch
    {
        Heading h => HeadingBlock(h),
        Code code => CodeBlock(code),
        List list => ListBlock(list),
        _ => Paragraph(((ParaBlock)block).Text),
    };

    private static SelectableTextBlock HeadingBlock(Heading h)
    {
        var tb = NewTextBlock();
        tb.FontWeight = FontWeight.Bold;
        tb.FontSize = h.Level switch { 1 => 18, 2 => 16, _ => 14.5 };
        tb.Margin = new Thickness(0, 4, 0, 0);
        AddInline(tb, h.Text, bold: true);
        return tb;
    }

    private static Control CodeBlock(Code code)
    {
        var tb = NewTextBlock();
        tb.FontFamily = CodeFont;
        tb.FontSize = 12.5;
        tb.Foreground = CodeForeground;
        tb.Inlines ??= new InlineCollection();
        var lines = code.Text.TrimEnd('\n').Split('\n');
        for (var i = 0; i < lines.Length; i++)
        {
            if (i > 0)
                tb.Inlines.Add(new LineBreak());
            tb.Inlines.Add(new Run(lines[i]));
        }
        return new Border
        {
            Background = BlockBackground,
            CornerRadius = new CornerRadius(6),
            Padding = new Thickness(10, 8),
            Child = tb,
        };
    }

    private static Control ListBlock(List list)
    {
        var panel = new StackPanel { Spacing = 3 };
        for (var i = 0; i < list.Items.Count; i++)
        {
            var tb = NewTextBlock();
            tb.Margin = new Thickness(14, 0, 0, 0);
            tb.Inlines ??= new InlineCollection();
            tb.Inlines.Add(new Run(list.Ordered ? $"{i + 1}.  " : "•  "));
            foreach (var inline in Inlines(list.Items[i]))
                tb.Inlines.Add(inline);
            panel.Children.Add(tb);
        }
        return panel;
    }

    private static SelectableTextBlock Paragraph(string text)
    {
        var tb = NewTextBlock();
        AddInline(tb, text, bold: false);
        return tb;
    }

    private static SelectableTextBlock NewTextBlock() =>
        new() { TextWrapping = TextWrapping.Wrap, HorizontalAlignment = HorizontalAlignment.Stretch };

    private static void AddInline(SelectableTextBlock tb, string text, bool bold)
    {
        tb.Inlines ??= new InlineCollection();
        foreach (var inline in Inlines(text, bold))
            tb.Inlines.Add(inline);
    }

    private static IEnumerable<Inline> Inlines(string text, bool bold = false, int depth = 0)
    {
        var plain = new StringBuilder();
        var i = 0;
        IEnumerable<Inline> Flush()
        {
            if (plain.Length > 0)
            {
                yield return MakeRun(plain.ToString(), bold, mono: false, link: false);
                plain.Clear();
            }
        }

        while (i < text.Length)
        {
            var ch = text[i];
            if (ch == '`')
            {
                var end = text.IndexOf('`', i + 1);
                if (end > i)
                {
                    foreach (var inline in Flush()) yield return inline;
                    yield return MakeRun(text[(i + 1)..end], bold: false, mono: true, link: false);
                    i = end + 1;
                    continue;
                }
            }
            else if (ch == '*' && i + 1 < text.Length && text[i + 1] == '*' && depth < 2)
            {
                var end = text.IndexOf("**", i + 2, StringComparison.Ordinal);
                if (end > 0)
                {
                    foreach (var inline in Flush()) yield return inline;
                    foreach (var inline in Inlines(text[(i + 2)..end], bold: true, depth + 1))
                        yield return inline;
                    i = end + 2;
                    continue;
                }
            }
            else if (ch == '*' && depth < 2 && (i == 0 || text[i - 1] != '`') && i + 1 < text.Length && !char.IsWhiteSpace(text[i + 1]))
            {
                var end = text.IndexOf('*', i + 1);
                if (end > i)
                {
                    foreach (var inline in Flush()) yield return inline;
                    foreach (var inline in Inlines(text[(i + 1)..end], bold, depth + 1))
                        yield return inline;
                    i = end + 1;
                    continue;
                }
            }
            else if (ch == '[')
            {
                var match = LinkRegex.Match(text, i);
                if (match.Success && match.Index == i)
                {
                    foreach (var inline in Flush()) yield return inline;
                    yield return MakeRun(match.Groups[1].Value, bold, mono: false, link: true);
                    i = match.Index + match.Length;
                    continue;
                }
            }
            plain.Append(ch);
            i++;
        }

        foreach (var inline in Flush()) yield return inline;
    }

    private static Run MakeRun(string text, bool bold, bool mono, bool link)
    {
        var run = new Run(text);
        if (mono)
        {
            run.FontFamily = CodeFont;
            run.FontSize = 12.5;
            run.Background = CodeBackground;
            run.Foreground = CodeForeground;
        }
        else if (link)
        {
            run.Foreground = LinkForeground;
            run.TextDecorations = TextDecorationCollection.Parse("Underline");
        }
        if (bold)
            run.FontWeight = FontWeight.Bold;
        return run;
    }

    private abstract record Block;

    private sealed record ParaBlock(string Text) : Block;

    private sealed record Heading(int Level, string Text) : Block;

    private sealed record Code(string Text) : Block;

    private sealed record List(bool Ordered, IReadOnlyList<string> Items) : Block;

    private static List<Block> Parse(string? text)
    {
        var blocks = new List<Block>();
        if (string.IsNullOrEmpty(text))
            return blocks;

        var lines = text.Split('\n');
        var para = new List<string>();
        var listItems = new List<string>();
        var listOrdered = false;
        var inList = false;
        var fenceChar = '\0';
        var code = new StringBuilder();

        void FlushPara()
        {
            if (para.Count > 0)
            {
                blocks.Add(new ParaBlock(string.Join('\n', para)));
                para.Clear();
            }
        }

        void FlushList()
        {
            if (inList)
            {
                blocks.Add(new List(listOrdered, listItems.ToArray()));
                listItems.Clear();
                inList = false;
            }
        }

        for (var i = 0; i < lines.Length;)
        {
            var line = lines[i].TrimEnd('\r');

            if (fenceChar != '\0')
            {
                if (line.TrimStart().StartsWith(fenceChar))
                {
                    blocks.Add(new Code(code.ToString()));
                    code.Clear();
                    fenceChar = '\0';
                }
                else
                {
                    code.Append(line).Append('\n');
                }
                i++;
                continue;
            }

            var trimmed = line.Trim();
            if (trimmed.StartsWith("```") || trimmed.StartsWith("~~~"))
            {
                FlushPara();
                FlushList();
                fenceChar = trimmed[0];
                i++;
                continue;
            }

            if (trimmed.Length == 0)
            {
                FlushPara();
                FlushList();
                i++;
                continue;
            }

            var level = 0;
            while (level < trimmed.Length && trimmed[level] == '#')
                level++;
            if (level is >= 1 and <= 6 && level < trimmed.Length && trimmed[level] == ' ')
            {
                FlushPara();
                FlushList();
                blocks.Add(new Heading(Math.Min(level, 3), trimmed[(level + 1)..].Trim()));
                i++;
                continue;
            }

            var orderedMatch = OrderedRegex.Match(trimmed);
            if (trimmed.StartsWith("- ") || trimmed.StartsWith("* ") || trimmed.StartsWith("+ ") || orderedMatch.Success)
            {
                FlushPara();
                var ordered = orderedMatch.Success;
                if (inList && ordered != listOrdered)
                    FlushList();
                listOrdered = ordered;
                inList = true;
                listItems.Add(ordered ? orderedMatch.Groups[1].Value : trimmed[2..].Trim());
                i++;
                continue;
            }

            FlushList();
            para.Add(trimmed);
            i++;
        }

        if (fenceChar != '\0')
            blocks.Add(new Code(code.ToString()));
        FlushPara();
        FlushList();
        return blocks;
    }
}
