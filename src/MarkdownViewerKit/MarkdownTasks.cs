using System.Text.RegularExpressions;

namespace MarkdownViewerKit;

/// <summary>
/// Task-list string operations behind the interactive preview: the preview-source
/// transform that feeds the checkbox pipeline and the source-text toggle. Task line
/// numbers are 1:1 with the source text, so a toggle raised for line N applies to
/// line N of the text the preview was rendered from.
/// </summary>
public static partial class MarkdownTasks
{
	[GeneratedRegex(@"^(\s*)[-*+]\s+\[([ xX])\]\s*(.*)$")]
	private static partial Regex TaskPrefix();

	// [ \t] rather than \s: \s crosses newlines, so a task preceded by a blank line
	// matched from that line's start and its computed line index came back one low
	// (the toggle then hit the blank line and silently no-oped).
	[GeneratedRegex(@"^([ \t]*)[-*+][ \t]+\[([ xX])\][ \t]*(.*)$", RegexOptions.Multiline)]
	private static partial Regex TaskBoxLine();

	/// <summary>A task item found in the markdown: its source line and checked state.</summary>
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
	public static string? Toggle(string text, int line)
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
}
