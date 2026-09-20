namespace MarkdownViewerKit;

/// <summary>Arguments for <see cref="MarkdownViewer.TaskToggled"/>: the zero-based
/// source line of the clicked task item and the checked state after the click.</summary>
public sealed class TaskToggledEventArgs(int line, bool isChecked) : EventArgs
{
	public int Line { get; } = line;

	public bool IsChecked { get; } = isChecked;
}
