using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Avalonia.Media;
using Avalonia.Styling;
using Avalonia.VisualTree;
using Markdown.Avalonia;
using MarkdownEngine = Markdown.Avalonia.Markdown;

namespace MarkdownViewerKit;

/// <summary>
/// Configurable markdown viewer built on the Markdown.Avalonia engine (ClassIsland
/// Tight fork). Ships an Obsidian-style dark/light theme, optional live task-list
/// checkboxes and a selection-overlay hit-test fix. All engine properties stay
/// available; the <c>Markwing*Brush</c> theme tokens can be overridden per instance
/// through <c>Resources</c>.
/// </summary>
/// <remarks>Bind <see cref="MarkdownScrollViewer.Markdown"/> OneWay: while a task
/// preview is active the property value holds the transformed preview text, so a
/// TwoWay binding would push it back into the source. Keep the source in the view
/// model and apply toggles there (<see cref="MarkdownTasks.Toggle"/>).</remarks>
public class MarkdownViewer : MarkdownScrollViewer
{
	private const string TaskKeyPrefix = "mtask-";

	private static readonly Uri ThemeUri =
		new("avares://MarkdownViewerKit.Avalonia/Themes/MarkdownTheme.axaml");

	private static readonly SolidColorBrush DefaultSelectionBrush = new(Color.Parse("#4D6E9BF7"));

	/// <summary>Renders task items (<c>- [ ]</c> / <c>- [x]</c>) as live checkboxes;
	/// clicks raise <see cref="TaskToggled"/> and the consumer updates the source text.</summary>
	public static readonly StyledProperty<bool> InteractiveTaskListsProperty =
		AvaloniaProperty.Register<MarkdownViewer, bool>(nameof(InteractiveTaskLists));

	/// <summary>Hides the list marker cell of a task row so the checkbox stands alone.</summary>
	public static readonly StyledProperty<bool> HideTaskListMarkersProperty =
		AvaloniaProperty.Register<MarkdownViewer, bool>(nameof(HideTaskListMarkers), defaultValue: true);

	/// <summary>Makes the engine's selection-overlay canvas input-transparent so it
	/// never blocks clicks on content under a selection band. Depends on the engine's
	/// internal template shape; disable on engine upgrades that move it.</summary>
	public static readonly StyledProperty<bool> SelectionHitTestFixProperty =
		AvaloniaProperty.Register<MarkdownViewer, bool>(nameof(SelectionHitTestFix), defaultValue: true);

	/// <summary>Checked task checkbox fill/border; falls back to the MarkwingAccentBrush token.</summary>
	public static readonly StyledProperty<IBrush?> TaskAccentBrushProperty =
		AvaloniaProperty.Register<MarkdownViewer, IBrush?>(nameof(TaskAccentBrush));

	/// <summary>Unchecked task checkbox border on pointer hover; falls back to the MarkwingAccentHoverBrush token.</summary>
	public static readonly StyledProperty<IBrush?> TaskAccentHoverBrushProperty =
		AvaloniaProperty.Register<MarkdownViewer, IBrush?>(nameof(TaskAccentHoverBrush));

	/// <summary>Raised when the user clicks a task checkbox. <see cref="TaskToggledEventArgs.Line"/>
	/// is the zero-based source line, <see cref="TaskToggledEventArgs.IsChecked"/> the state after the click.
	/// The checkbox paints no state of its own — update the source text (e.g. <see cref="MarkdownTasks.Toggle"/>)
	/// and the preview re-renders from it.</summary>
	public event EventHandler<TaskToggledEventArgs>? TaskToggled;

	private bool _renderingPreview;
	private bool _previewActive;
	private string? _source;
	private bool _selectionFixPending;

	public MarkdownViewer()
	{
		// Instance defaults; consumer attributes and bindings applied after the ctor win.
		UseResource = true;
		SelectionEnabled = true;
		SelectionBrush = DefaultSelectionBrush;
		SaveScrollValueWhenContentUpdated = true;
		Styles.Add((Styles)AvaloniaXamlLoader.Load(ThemeUri));
		AttachedToVisualTree += OnVisualTreeAttached;
	}

	public bool InteractiveTaskLists
	{
		get => GetValue(InteractiveTaskListsProperty);
		set => SetValue(InteractiveTaskListsProperty, value);
	}

	public bool HideTaskListMarkers
	{
		get => GetValue(HideTaskListMarkersProperty);
		set => SetValue(HideTaskListMarkersProperty, value);
	}

	public bool SelectionHitTestFix
	{
		get => GetValue(SelectionHitTestFixProperty);
		set => SetValue(SelectionHitTestFixProperty, value);
	}

	public IBrush? TaskAccentBrush
	{
		get => GetValue(TaskAccentBrushProperty);
		set => SetValue(TaskAccentBrushProperty, value);
	}

	public IBrush? TaskAccentHoverBrush
	{
		get => GetValue(TaskAccentHoverBrushProperty);
		set => SetValue(TaskAccentHoverBrushProperty, value);
	}

	protected override void OnPropertyChanged(AvaloniaPropertyChangedEventArgs change)
	{
		base.OnPropertyChanged(change);
		if (_renderingPreview)
		{
			return;
		}

		if (change.Property == MarkdownProperty)
		{
			if (InteractiveTaskLists)
			{
				RenderWithTasks(change.GetNewValue<string>());
			}
		}
		else if (change.Property == InteractiveTaskListsProperty)
		{
			if (InteractiveTaskLists)
			{
				RenderWithTasks(GetCurrentValueString());
			}
			else if (_previewActive && _source is not null)
			{
				SetCurrentValue(MarkdownProperty, _source);
				_previewActive = false;
			}
		}
		else if (change.Property == SelectionHitTestFixProperty && change.GetNewValue<bool>())
		{
			DisableSelectionOverlayHitTest();
			ScheduleSelectionHitTestFix();
		}
	}

	private void OnVisualTreeAttached(object? sender, VisualTreeAttachmentEventArgs e)
	{
		ScheduleSelectionHitTestFix();

		// The engine re-parses on re-attach, consuming CascadeResources — re-render
		// from the source so the checkbox population always precedes a parse.
		if (InteractiveTaskLists)
		{
			RenderWithTasks(_source ?? GetCurrentValueString());
		}
	}

	/// <summary>Transforms the source, populates one checkbox per task anchor and feeds
	/// the preview to the engine. The base property callback parses inside the same
	/// change notification (on the raw source), so a source update costs two parses;
	/// the second one is the one the user sees.</summary>
	private void RenderWithTasks(string? source)
	{
		if (Engine is not MarkdownEngine engine)
		{
			return;
		}

		_renderingPreview = true;
		_source = source;
		try
		{
			var (preview, tasks) = MarkdownTasks.BuildPreview(source ?? string.Empty);
			var resources = engine.CascadeResources.Owner;
			foreach (var key in resources.Keys.OfType<string>()
				.Where(k => k.StartsWith(TaskKeyPrefix, StringComparison.Ordinal)).ToList())
			{
				resources.Remove(key);
			}

			foreach (var task in tasks)
			{
				resources[$"{TaskKeyPrefix}{task.Line}"] = new TaskCheckBox(this, task.Line, task.IsChecked);
			}

			SetCurrentValue(MarkdownProperty, preview);
			_previewActive = true;
		}
		finally
		{
			_renderingPreview = false;
		}

		ScheduleSelectionHitTestFix();
	}

	private string GetCurrentValueString() => (string?)GetValue(MarkdownProperty) ?? string.Empty;

	/// <summary>The overlay canvas is template-level and only exists after the first
	/// layout pass, so the fix runs on the next LayoutUpdated instead of inline.</summary>
	private void ScheduleSelectionHitTestFix()
	{
		if (!SelectionHitTestFix || _selectionFixPending)
		{
			return;
		}

		_selectionFixPending = true;
		LayoutUpdated += OnSelectionFixLayoutUpdated;
	}

	private void OnSelectionFixLayoutUpdated(object? sender, EventArgs e)
	{
		LayoutUpdated -= OnSelectionFixLayoutUpdated;
		_selectionFixPending = false;
		DisableSelectionOverlayHitTest();
	}

	/// <summary>The engine paints selection bands on an overlay canvas stacked above the
	/// document; its rectangles survive unselect and intercept clicks, making a selected
	/// line's checkbox untoggleable. Input-transparent fixes that while the bands keep
	/// rendering. The canvas has no logical parent — match it by its visual chain
	/// (canvas → [wrappers] → this viewer's internal ScrollViewer).</summary>
	private void DisableSelectionOverlayHitTest()
	{
		foreach (var canvas in this.GetVisualDescendants().OfType<Canvas>())
		{
			var node = canvas.GetVisualParent();
			while (node is not null && node is not ScrollViewer && !ReferenceEquals(node, this))
			{
				node = node.GetVisualParent();
			}

			if (node is ScrollViewer internalScroll && ReferenceEquals(internalScroll.GetVisualParent(), this))
			{
				canvas.IsHitTestVisible = false;
			}
		}
	}

	internal void NotifyTaskToggled(int line, bool isChecked)
		=> TaskToggled?.Invoke(this, new TaskToggledEventArgs(line, isChecked));

	internal static void HideListMarker(Visual start)
	{
		// Task items keep the list structure for indentation, but their bullet glyph is
		// noise next to the checkbox: hide the row's marker column cell once the
		// checkbox lands in the visual tree.
		var child = start;
		var node = start.GetVisualParent();
		while (node is not null)
		{
			if (node is Grid { Classes.Count: > 0 } grid && grid.Classes.Contains("List"))
			{
				var row = Grid.GetRow((Control)child);
				foreach (var marker in grid.Children)
				{
					if (Grid.GetRow(marker) == row && Grid.GetColumn(marker) == 0)
					{
						marker.IsVisible = false;
					}
				}

				return;
			}

			child = node;
			node = node.GetVisualParent();
		}
	}
}
