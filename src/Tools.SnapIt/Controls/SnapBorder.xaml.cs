using Avalonia.Controls;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;
using Point = Avalonia.Point;
using Size = Avalonia.Size;

namespace Tools.SnapIt.Controls;

public partial class SnapBorder : UserControl
{
	public const int THICKNESS = 12;
	public const int THICKNESSHALF = 6;

	public static readonly StyledProperty<SnapAreaTheme> SnapThemeProperty =
		AvaloniaProperty.Register<SnapBorder, SnapAreaTheme>(
			nameof(SnapTheme),
			defaultBindingMode: BindingMode.TwoWay);

	public SnapAreaTheme SnapTheme
	{
		get => GetValue(SnapThemeProperty);
		set => SetValue(SnapThemeProperty, value);
	}

	public static readonly StyledProperty<SplitDirection> SplitDirectionProperty =
		AvaloniaProperty.Register<SnapBorder, SplitDirection>(
			nameof(SplitDirection),
			defaultValue: SplitDirection.Vertical,
			defaultBindingMode: BindingMode.TwoWay);

	public SplitDirection SplitDirection
	{
		get => GetValue(SplitDirectionProperty);
		set => SetValue(SplitDirectionProperty, value);
	}

	static SnapBorder()
	{
		SnapThemeProperty.Changed.AddClassHandler<SnapBorder>((snapBorder, e) =>
		{
			if (snapBorder.SnapTheme != null)
			{
				snapBorder.Border.Background = snapBorder.SnapTheme.OverlayBrush;
				snapBorder.ReferenceBorder.Background = snapBorder.SnapTheme.BorderBrush;
				snapBorder.ReferenceBorder.Opacity = snapBorder.SnapTheme.Opacity;
				snapBorder.Opacity = snapBorder.SnapTheme.Opacity;
			}
		});

		SplitDirectionProperty.Changed.AddClassHandler<SnapBorder>((snapBorder, e) =>
		{
			snapBorder.SplitDirection = e.NewValue is SplitDirection sd ? sd : SplitDirection.Vertical;
		});
	}

	public bool IsDraggable { get; set; } = true;
	public SnapControl SnapControl { get; }
	public Line LayoutLine { get; internal set; }

	private Point _positionInBlock;

	public SnapBorder(SnapControl snapControl, SnapAreaTheme theme)
	{
		InitializeComponent();
		SnapControl = snapControl;
		SnapTheme = theme;

		HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Left;
		VerticalAlignment = Avalonia.Layout.VerticalAlignment.Top;
	}

	public void SetPos(Point point, Size size, SplitDirection splitDirection)
	{
		Margin = new Thickness(point.X, point.Y, 0, 0);
		SplitDirection = splitDirection;

		if (IsDraggable)
		{
			if (SplitDirection == SplitDirection.Vertical)
			{
				Width = THICKNESS;
				Height = size.Height;

				Margin = new Thickness(Margin.Left - THICKNESSHALF, Margin.Top, 0, 0);

				Border.Cursor = new Cursor(StandardCursorType.SizeWestEast);
				ReferenceBorder.Cursor = new Cursor(StandardCursorType.SizeWestEast);
				Border.Width = THICKNESS;
				Border.VerticalAlignment = Avalonia.Layout.VerticalAlignment.Stretch;

				ReferenceBorder.Width = 1;
				ReferenceBorder.VerticalAlignment = Avalonia.Layout.VerticalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(THICKNESSHALF, 0, 0, 0);
			}
			else
			{
				Width = size.Width;
				Height = THICKNESS;

				Margin = new Thickness(Margin.Left, Margin.Top - THICKNESSHALF, 0, 0);

				Border.Cursor = new Cursor(StandardCursorType.SizeNorthSouth);
				ReferenceBorder.Cursor = new Cursor(StandardCursorType.SizeNorthSouth);
				Border.Height = THICKNESS;
				Border.HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Stretch;

				ReferenceBorder.Height = 1;
				ReferenceBorder.HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(0, THICKNESSHALF, 0, 0);
			}
		}
		else
		{
			ReferenceBorder.IsVisible = false;
			if (SplitDirection == SplitDirection.Vertical)
			{
				Width = THICKNESS;
				Height = size.Height;

				Border.Width = THICKNESS;
				Border.VerticalAlignment = Avalonia.Layout.VerticalAlignment.Stretch;

				ReferenceBorder.Width = 1;
				ReferenceBorder.VerticalAlignment = Avalonia.Layout.VerticalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(Bounds.Width / 2, 0, 0, 0);
			}
			else
			{
				Width = size.Width;
				Height = THICKNESS;

				Border.Height = THICKNESS;
				Border.HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Stretch;

				ReferenceBorder.Height = 1;
				ReferenceBorder.HorizontalAlignment = Avalonia.Layout.HorizontalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(0, Bounds.Height / 2, 0, 0);
			}
		}
	}

	public Rect GetRect()
	{
		return new Rect(
			new Point(Margin.Left, Margin.Top),
			new Size(
				Bounds.Width == 0 ? Width : Bounds.Width,
				Bounds.Height == 0 ? Height : Bounds.Height));
	}

	public Line GetLine()
	{
		var line = SplitDirection == SplitDirection.Vertical ?
			new Line
			{
				Start = new Graphics.Point((float)(Margin.Left + ReferenceBorder.Margin.Left), (float)(Margin.Top + ReferenceBorder.Margin.Top)),
				End = new Graphics.Point((float)(Margin.Left + ReferenceBorder.Margin.Left), (float)(Margin.Top + ReferenceBorder.Margin.Top + Height))
			} :
			new Line
			{
				Start = new Graphics.Point((float)(Margin.Left + ReferenceBorder.Margin.Left), (float)(Margin.Top + ReferenceBorder.Margin.Top)),
				End = new Graphics.Point((float)(Margin.Left + ReferenceBorder.Margin.Left + Width), (float)(Margin.Top + ReferenceBorder.Margin.Top))
			};

		line.Point = new Graphics.Point(line.Start.X, line.Start.Y);
		line.SplitDirection = SplitDirection;
		line.Size = new Graphics.Size(
		System.Math.Abs(line.Start.X - line.End.X),
		System.Math.Abs(line.Start.Y - line.End.Y));

		return line;
	}
}
