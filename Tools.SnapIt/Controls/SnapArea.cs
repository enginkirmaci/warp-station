using System.Windows.Controls;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Graphics;
using Point = System.Windows.Point;

namespace Tools.SnapIt.Controls;

public class SnapArea : Control
{
	public SnapControl SnapControl { get; set; }

	public Thickness AreaPadding
	{
		get => (Thickness)GetValue(AreaPaddingProperty);
		set => SetValue(AreaPaddingProperty, value);
	}

	public static readonly DependencyProperty AreaPaddingProperty
	 = DependencyProperty.Register("AreaPadding", typeof(Thickness), typeof(SnapArea),
	   new FrameworkPropertyMetadata()
	   {
		   DefaultValue = new Thickness(0),
		   BindsTwoWayByDefault = true,
		   PropertyChangedCallback = new PropertyChangedCallback(AreaPaddingPropertyChanged)
	   });

	private static void AreaPaddingPropertyChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
	{
		var snapAreaEditor = (SnapArea)d;
		snapAreaEditor.AreaPadding = (Thickness)e.NewValue;
	}

	public SnapAreaTheme Theme
	{
		get => (SnapAreaTheme)GetValue(ThemeProperty);
		set => SetValue(ThemeProperty, value);
	}

	public static readonly DependencyProperty ThemeProperty
	 = DependencyProperty.Register("Theme", typeof(SnapAreaTheme), typeof(SnapArea),
	   new FrameworkPropertyMetadata()
	   {
		   BindsTwoWayByDefault = true,
		   PropertyChangedCallback = new PropertyChangedCallback(ThemePropertyChanged)
	   });

	private static void ThemePropertyChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
	{
		var snapArea = (SnapArea)d;
		snapArea.Theme = (SnapAreaTheme)e.NewValue;
	}

	public void NormalStyle()
	{
		var area = this.FindChild<Grid>("Area");
		if (area != null)
		{
			area.Background = Theme.OverlayBrush;
		}
	}

	public void OnHoverStyle()
	{
		var area = this.FindChild<Grid>("Area");
		if (area != null)
		{
			area.Background = Theme.HighlightBrush;
		}
	}

	public Rectangle ScreenSnapArea(Dpi dpi)
	{
		var topLeft = PointToScreen(new Point(SnapControl.AreaPadding, SnapControl.AreaPadding));

		var bottomRight = PointToScreen(new Point(ActualWidth - SnapControl.AreaPadding, ActualHeight - SnapControl.AreaPadding));

		return new Rectangle(
		   (int)topLeft.X,
		   (int)topLeft.Y,
		   (int)bottomRight.X,
		   (int)bottomRight.Y,
		   dpi);
	}
}