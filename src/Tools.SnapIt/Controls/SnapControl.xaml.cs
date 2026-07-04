using Avalonia.Controls;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Math.FindRectangle;
using Point = Avalonia.Point;
using Size = Avalonia.Size;

namespace Tools.SnapIt.Controls;

public partial class SnapControl : UserControl
{
	private readonly SnapBorder topBorder;
	private readonly SnapBorder bottomBorder;
	private readonly SnapBorder leftBorder;
	private readonly SnapBorder rightBorder;

	private double overlayMargin = 0;
	private bool firstLoad = true;
	private string currentName;
	private int currentAreaPadding;
	private List<Line> currentLayoutLines;
	private List<LayoutOverlay> currentLayoutOverlays;

	public static readonly StyledProperty<int> AreaPaddingProperty =
		AvaloniaProperty.Register<SnapControl, int>(
			nameof(AreaPadding),
			defaultValue: 0,
			defaultBindingMode: BindingMode.TwoWay);

	public int AreaPadding
	{
		get => GetValue(AreaPaddingProperty);
		set => SetValue(AreaPaddingProperty, value);
	}

	public static readonly StyledProperty<bool> IsOverlayVisibleProperty =
		AvaloniaProperty.Register<SnapControl, bool>(
			nameof(IsOverlayVisible),
			defaultValue: true,
			defaultBindingMode: BindingMode.TwoWay);

	public bool IsOverlayVisible
	{
		get => GetValue(IsOverlayVisibleProperty);
		set => SetValue(IsOverlayVisibleProperty, value);
	}

	public static readonly StyledProperty<SnapAreaTheme> SnapThemeProperty =
		AvaloniaProperty.Register<SnapControl, SnapAreaTheme>(
			nameof(SnapTheme),
			defaultValue: new SnapAreaTheme(),
			defaultBindingMode: BindingMode.TwoWay);

	public SnapAreaTheme SnapTheme
	{
		get => GetValue(SnapThemeProperty);
		set => SetValue(SnapThemeProperty, value);
	}

	public static readonly StyledProperty<bool> IsPreviewProperty =
		AvaloniaProperty.Register<SnapControl, bool>(
			nameof(IsPreview),
			defaultValue: false,
			defaultBindingMode: BindingMode.TwoWay);

	public bool IsPreview
	{
		get => GetValue(IsPreviewProperty);
		set => SetValue(IsPreviewProperty, value);
	}

	public static readonly StyledProperty<Layout> LayoutProperty =
		AvaloniaProperty.Register<SnapControl, Layout>(
			nameof(Layout),
			defaultBindingMode: BindingMode.TwoWay);

	public Layout Layout
	{
		get => GetValue(LayoutProperty);
		set => SetValue(LayoutProperty, value);
	}

	static SnapControl()
	{
		AreaPaddingProperty.Changed.AddClassHandler<SnapControl>((snapControl, e) =>
		{
			snapControl.AreaPadding = e.NewValue is int v ? v : 0;
			var snapAreas = snapControl.FindChildren<SnapArea>();
			foreach (var snapArea in snapAreas)
			{
				snapArea.AreaPadding = new Thickness(snapControl.AreaPadding);
			}
		});

		IsOverlayVisibleProperty.Changed.AddClassHandler<SnapControl>((snapControl, e) =>
		{
			snapControl.IsOverlayVisible = e.NewValue is bool b && b;
			snapControl.MainOverlay.IsVisible = snapControl.IsOverlayVisible;
		});

		SnapThemeProperty.Changed.AddClassHandler<SnapControl>((snapControl, e) =>
		{
			if (snapControl.SnapTheme != null)
			{
				var snapAreas = snapControl.FindChildren<SnapArea>();
				foreach (var snapArea in snapAreas)
				{
					snapArea.SnapTheme = snapControl.SnapTheme;
				}

				var snapBorders = snapControl.FindChildren<SnapBorder>();
				foreach (var snapBorder in snapBorders)
				{
					snapBorder.SnapTheme = snapControl.SnapTheme;
				}

				var snapOverlays = snapControl.FindChildren<SnapOverlay>();
				foreach (var snapOverlay in snapOverlays)
				{
					snapOverlay.SnapTheme = snapControl.SnapTheme;
				}

				var snapFullOverlays = snapControl.FindChildren<SnapFullOverlay>();
				foreach (var snapFullOverlay in snapFullOverlays)
				{
					snapFullOverlay.SnapTheme = snapControl.SnapTheme;
				}
			}
		});

		IsPreviewProperty.Changed.AddClassHandler<SnapControl>((snapControl, e) =>
		{
			snapControl.IsPreview = e.NewValue is bool b && b;
		});

		LayoutProperty.Changed.AddClassHandler<SnapControl>((snapControl, e) =>
		{
			var layout = e.NewValue as Layout;
			snapControl.LoadLayout(layout);
		});
	}

	public SnapControl()
	{
		InitializeComponent();

		SnapTheme = new SnapAreaTheme();

		MainGrid.IsVisible = false;

		topBorder = new SnapBorder(this, SnapTheme) { IsDraggable = false };
		bottomBorder = new SnapBorder(this, SnapTheme) { IsDraggable = false };
		leftBorder = new SnapBorder(this, SnapTheme) { IsDraggable = false };
		rightBorder = new SnapBorder(this, SnapTheme) { IsDraggable = false };

		SizeChanged += SnapControl_SizeChanged;
	}

	private void SnapControl_SizeChanged(object sender, SizeChangedEventArgs e)
	{
		AdoptToScreen();
	}

	public void AddBorder(SnapBorder snapBorder)
	{
		MainGrid.Children.Add(snapBorder);
		GenerateSnapAreas();
	}

	public void LoadLayout(Layout layout)
	{
		if (firstLoad)
		{
			firstLoad = false;
			currentName = layout.Name;
			currentAreaPadding = layout.AreaPadding;
			currentLayoutLines = new List<Line>(layout.LayoutLines);
			currentLayoutOverlays = new List<LayoutOverlay>(layout.LayoutOverlays);
		}
		;

		MainGrid.Children.Clear();
		MainFullOverlay.Children.Clear();
		MainOverlay.Children.Clear();

		MainGrid.Children.Add(topBorder);
		MainGrid.Children.Add(bottomBorder);
		MainGrid.Children.Add(leftBorder);
		MainGrid.Children.Add(rightBorder);

		if (layout != null)
		{
			AreaPadding = layout.AreaPadding;

			foreach (var layoutLine in layout.LayoutLines)
			{
				var snapBorder = new SnapBorder(this, SnapTheme)
				{
					LayoutLine = layoutLine
				};

				AddBorder(snapBorder);
			}

			if (layout.LayoutOverlays != null)
			{
				foreach (var layoutOverlay in layout.LayoutOverlays)
				{
					var fullOverlay = new SnapFullOverlay(SnapTheme)
					{
						LayoutOverlay = layoutOverlay
					};

					var overlay = new SnapOverlay(SnapTheme, fullOverlay)
					{
						LayoutOverlay = layoutOverlay,
					};

					MainFullOverlay.Children.Add(fullOverlay);
					MainOverlay.Children.Add(overlay);
				}
			}
		}

		AdoptToScreen();
	}

	private void AdoptToScreen()
	{
		topBorder.SetPos(new Point(0, -SnapBorder.THICKNESSHALF), new Size(MainGrid.Bounds.Width, 0), SplitDirection.Horizontal);
		bottomBorder.SetPos(new Point(0, MainGrid.Bounds.Height - SnapBorder.THICKNESSHALF), new Size(MainGrid.Bounds.Width, 0), SplitDirection.Horizontal);
		leftBorder.SetPos(new Point(-SnapBorder.THICKNESSHALF, 0), new Size(0, MainGrid.Bounds.Height), SplitDirection.Vertical);
		rightBorder.SetPos(new Point(MainGrid.Bounds.Width - SnapBorder.THICKNESSHALF, 0), new Size(0, MainGrid.Bounds.Height), SplitDirection.Vertical);

		if (Layout != null && Bounds.Width != 0)
		{
			var factorX = Bounds.Width / Layout.Size.Width;
			var factorY = Bounds.Height / Layout.Size.Height;

			// Walk once and filter inline instead of FindChildren + .Where (which
			// chains a second iterator over the recursive visual-tree walk).
			foreach (var border in this.FindChildren<SnapBorder>())
			{
				if (!border.IsDraggable)
				{
					continue;
				}

				if (border.LayoutLine != null)
				{
					var newPoint = new Point(
						border.LayoutLine.Point.X * factorX,
						border.LayoutLine.Point.Y * factorY);
					var newSize = new Size(
						border.LayoutLine.Size.Width * factorX,
						border.LayoutLine.Size.Height * factorY);

					border.SetPos(newPoint, newSize, border.LayoutLine.SplitDirection);
				}

				if (IsPreview)
				{
					AreaPadding = (int)(Layout.AreaPadding * factorX);
				}
			}

			var fullOverlays = this.FindChildren<SnapFullOverlay>();
			foreach (var fullOverlay in fullOverlays)
			{
				if (fullOverlay.LayoutOverlay != null)
				{
					var newPoint = new Point(
						fullOverlay.LayoutOverlay.Point.X * factorX,
						fullOverlay.LayoutOverlay.Point.Y * factorY);
					var newSize = new Size(
						fullOverlay.LayoutOverlay.Size.Width * factorX,
						fullOverlay.LayoutOverlay.Size.Height * factorY);

					fullOverlay.SetPos(newPoint, newSize);
				}
			}

			var overlays = this.FindChildren<SnapOverlay>();
			foreach (var overlay in overlays)
			{
				if (overlay.LayoutOverlay != null)
				{
					if (overlay.LayoutOverlay.MiniOverlay != null)
					{
						var miniPoint = new Point(
							overlay.LayoutOverlay.MiniOverlay.Point.X * factorX,
							overlay.LayoutOverlay.MiniOverlay.Point.Y * factorY);

						var miniSize = new Size(
							overlay.LayoutOverlay.MiniOverlay.Size.Width * factorX,
							overlay.LayoutOverlay.MiniOverlay.Size.Height * factorY);

						overlay.SetPos(new LayoutOverlay { Point = miniPoint.Convert(), Size = miniSize.Convert() });
					}
					else
					{
						overlay.SetPos(null);
					}
				}
			}
		}

		GenerateSnapAreas();
	}

	public void GenerateSnapAreas()
	{
		MainAreas.Children.Clear();

		// Bail early during initial layout passes where bounds aren't measured
		// yet — there's nothing meaningful to generate and this avoids a wasted
		// rebuild once real bounds arrive.
		if (Bounds.Width == 0 || Bounds.Height == 0)
		{
			return;
		}

		// Collect borders once and filter inline rather than chaining a .Where()
		// over the FindChildren iterator, which would allocate a second iterator.
		var draggableBorders = new List<SnapBorder>();
		foreach (var border in this.FindChildren<SnapBorder>())
		{
			if (border.IsDraggable)
			{
				draggableBorders.Add(border);
			}
		}

		Math.FindRectangle.Settings settings = new Math.FindRectangle.Settings
		{
			Size = new System.Drawing.Size((int)Bounds.Width, (int)Bounds.Height),
			Segments = []
		};

		var newLayoutLines = new List<Line>();

		foreach (var border in draggableBorders)
		{
			var line = border.GetLine();

			newLayoutLines.Add(line);

			settings.Segments.Add(new Segment
			{
				Location = new System.Drawing.Point((int)line.Start.X, (int)line.Start.Y),
				EndLocation = new System.Drawing.Point((int)line.End.X, (int)line.End.Y),
				Orientation = line.SplitDirection
			});
		}

		settings.Calculate();

		var rectangles = settings.GetRectangles();

		foreach (var rectangle in rectangles)
		{
			var snapArea = new SnapArea()
			{
				Margin = new Thickness(rectangle.TopLeft.X, rectangle.TopLeft.Y, 0, 0),
				Width = rectangle.Width,
				Height = rectangle.Height,
				SnapControl = this,
				SnapTheme = SnapTheme,
				AreaPadding = new Thickness(AreaPadding)
			};

			MainAreas.Children.Add(snapArea);
		}
	}
}
