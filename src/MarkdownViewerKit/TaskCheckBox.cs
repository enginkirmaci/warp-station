using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.VisualTree;

namespace MarkdownViewerKit;

/// <summary>
/// A plain hand-drawn box rather than a CheckBox: the checkbox sits inside the
/// markdown viewer's template tree where theme resolution is unpredictable, and
/// state is owned by the source text anyway — the box is a clickable picture that
/// relays clicks to the viewer's <see cref="MarkdownViewer.TaskToggled"/> event.
/// Accent brushes are snapshotted at render time; property changes apply on the
/// next render.
/// </summary>
internal sealed class TaskCheckBox : Border
{
	private const string TickGeometry = "M 3,7.6 L 6.2,10.8 L 12,4.4";

	private const string DefaultAccent = "#8A5CF5";
	private const string DefaultAccentHover = "#C9B8F8";
	private const string DefaultIdleBorder = "#8A8A8A";

	private readonly MarkdownViewer _owner;
	private readonly int _line;
	private readonly bool _isChecked;
	private readonly Avalonia.Controls.Shapes.Path _tick;
	private readonly IBrush _accent;
	private readonly IBrush _accentHover;
	private readonly IBrush _idleBorder;

	public TaskCheckBox(MarkdownViewer owner, int line, bool isChecked)
	{
		_owner = owner;
		_line = line;
		_isChecked = isChecked;
		_accent = owner.TaskAccentBrush ?? FindToken(owner, "MarkwingAccentBrush") ?? Solid(DefaultAccent);
		_accentHover = owner.TaskAccentHoverBrush ?? FindToken(owner, "MarkwingAccentHoverBrush") ?? Solid(DefaultAccentHover);
		_idleBorder = Solid(DefaultIdleBorder);

		_tick = new Avalonia.Controls.Shapes.Path
		{
			Data = Geometry.Parse(TickGeometry),
			Stroke = new SolidColorBrush(Colors.White),
			StrokeThickness = 1.8,
			IsVisible = isChecked,
		};
		Width = 15;
		Height = 15;
		CornerRadius = new CornerRadius(3.5);
		BorderThickness = new Thickness(1.4);
		BorderBrush = isChecked ? _accent : _idleBorder;
		Background = isChecked ? _accent : Brushes.Transparent;
		Cursor = new Cursor(StandardCursorType.Hand);
		Child = _tick;

		PointerEntered += (_, _) =>
		{
			if (!_tick.IsVisible)
			{
				BorderBrush = _accentHover;
			}
		};
		PointerExited += (_, _) =>
		{
			if (!_tick.IsVisible)
			{
				BorderBrush = _idleBorder;
			}
		};
		PointerPressed += (_, e) =>
		{
			e.Handled = true;
			_owner.NotifyTaskToggled(_line, !_isChecked);
		};
		AttachedToVisualTree += (_, _) =>
		{
			if (_owner.HideTaskListMarkers)
			{
				MarkdownViewer.HideListMarker(this);
			}
		};
	}

	private static IBrush? FindToken(MarkdownViewer owner, string key)
		=> owner.TryFindResource(key, out var value) && value is IBrush brush ? brush : null;

	private static IBrush Solid(string hex) => new SolidColorBrush(Color.Parse(hex));
}
