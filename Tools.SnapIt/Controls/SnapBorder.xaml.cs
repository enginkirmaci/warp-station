using System.Windows.Controls;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;
using Point = System.Windows.Point;
using Size = System.Windows.Size;

namespace Tools.SnapIt.Controls;

public partial class SnapBorder : UserControl
{
	public const int THICKNESS = 12;
	public const int THICKNESSHALF = 6;

	public SnapAreaTheme Theme
	{
		get => (SnapAreaTheme)GetValue(ThemeProperty);
		set => SetValue(ThemeProperty, value);
	}

	public static readonly DependencyProperty ThemeProperty
	 = DependencyProperty.Register("Theme", typeof(SnapAreaTheme), typeof(SnapBorder),
	   new FrameworkPropertyMetadata()
	   {
		   BindsTwoWayByDefault = true,
		   PropertyChangedCallback = new PropertyChangedCallback(ThemePropertyChanged)
	   });

	private static void ThemePropertyChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
	{
		var snapBorder = (SnapBorder)d;
		snapBorder.Theme = (SnapAreaTheme)e.NewValue;

		if (snapBorder.Theme != null)
		{
			snapBorder.Border.Background = snapBorder.Theme.OverlayBrush;
			snapBorder.ReferenceBorder.Background = snapBorder.Theme.BorderBrush;
			snapBorder.ReferenceBorder.Opacity = snapBorder.Theme.Opacity;
			snapBorder.Opacity = snapBorder.Theme.Opacity;
		}
	}

	public SplitDirection SplitDirection
	{
		get => (SplitDirection)GetValue(SplitDirectionProperty);
		set => SetValue(SplitDirectionProperty, value);
	}

	public static readonly DependencyProperty SplitDirectionProperty
	 = DependencyProperty.Register("SplitDirection", typeof(SplitDirection), typeof(SnapBorder),
	   new FrameworkPropertyMetadata()
	   {
		   BindsTwoWayByDefault = true,
		   DefaultValue = SplitDirection.Vertical,
		   PropertyChangedCallback = new PropertyChangedCallback(SplitDirectionPropertyChanged)
	   });

	private static void SplitDirectionPropertyChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
	{
		var snapBorder = (SnapBorder)d;
		snapBorder.SplitDirection = (SplitDirection)e.NewValue;
	}

	public bool IsDraggable { get; set; } = true;
	public SnapControl SnapControl { get; }
	public Line LayoutLine { get; internal set; }

	private Point _positionInBlock;

	public SnapBorder(SnapControl snapControl, SnapAreaTheme theme)
	{
		InitializeComponent();
		SnapControl = snapControl;
		Theme = theme;

		HorizontalAlignment = HorizontalAlignment.Left;
		VerticalAlignment = VerticalAlignment.Top;
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

				Border.Cursor = Cursors.SizeWE;
				ReferenceBorder.Cursor = Cursors.SizeWE;
				Border.Width = THICKNESS;
				Border.VerticalAlignment = VerticalAlignment.Stretch;

				ReferenceBorder.Width = 1;
				ReferenceBorder.VerticalAlignment = VerticalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(THICKNESSHALF, 0, 0, 0);
			}
			else
			{
				Width = size.Width;
				Height = THICKNESS;

				Margin = new Thickness(Margin.Left, Margin.Top - THICKNESSHALF, 0, 0);

				Border.Cursor = Cursors.SizeNS;
				ReferenceBorder.Cursor = Cursors.SizeNS;
				Border.Height = THICKNESS;
				Border.HorizontalAlignment = HorizontalAlignment.Stretch;

				ReferenceBorder.Height = 1;
				ReferenceBorder.HorizontalAlignment = HorizontalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(0, THICKNESSHALF, 0, 0);
			}
		}
		else
		{
			ReferenceBorder.Visibility = Visibility.Collapsed;
			if (SplitDirection == SplitDirection.Vertical)
			{
				Width = THICKNESS;
				Height = size.Height;

				Border.Width = THICKNESS;
				Border.VerticalAlignment = VerticalAlignment.Stretch;

				ReferenceBorder.Width = 1;
				ReferenceBorder.VerticalAlignment = VerticalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(ActualWidth / 2, 0, 0, 0);
			}
			else
			{
				Width = size.Width;
				Height = THICKNESS;

				Border.Height = THICKNESS;
				Border.HorizontalAlignment = HorizontalAlignment.Stretch;

				ReferenceBorder.Height = 1;
				ReferenceBorder.HorizontalAlignment = HorizontalAlignment.Stretch;
				ReferenceBorder.Margin = new Thickness(0, ActualHeight / 2, 0, 0);
			}
		}
	}

	public Rect GetRect()
	{
		return new Rect(
			new Point(Margin.Left, Margin.Top),
			new Size(
				ActualWidth == 0 ? Width : ActualWidth,
				ActualHeight == 0 ? Height : ActualHeight));
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