using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Graphics;
using Point = Avalonia.Point;

namespace Tools.SnapIt.Controls;

public class SnapArea : TemplatedControl
{
	public SnapControl SnapControl { get; set; }

	public static readonly StyledProperty<Thickness> AreaPaddingProperty =
		AvaloniaProperty.Register<SnapArea, Thickness>(
			nameof(AreaPadding),
			defaultValue: new Thickness(0),
			defaultBindingMode: BindingMode.TwoWay);

	public Thickness AreaPadding
	{
		get => GetValue(AreaPaddingProperty);
		set => SetValue(AreaPaddingProperty, value);
	}

	public static readonly StyledProperty<SnapAreaTheme> SnapThemeProperty =
		AvaloniaProperty.Register<SnapArea, SnapAreaTheme>(
			nameof(SnapTheme),
			defaultValue: new SnapAreaTheme(),
			defaultBindingMode: BindingMode.TwoWay);

	public SnapAreaTheme SnapTheme
	{
		get => GetValue(SnapThemeProperty);
		set => SetValue(SnapThemeProperty, value);
	}

	static SnapArea()
	{
		AreaPaddingProperty.Changed.AddClassHandler<SnapArea>((snapArea, e) =>
		{
			snapArea.AreaPadding = e.NewValue is Thickness t ? t : new Thickness(0);
		});

		SnapThemeProperty.Changed.AddClassHandler<SnapArea>((snapArea, e) =>
		{
			snapArea.SnapTheme = e.NewValue as SnapAreaTheme;
		});
	}

	public SnapArea()
	{
		Name = $"snaparea_{Guid.NewGuid():N}";
	}

	public void NormalStyle()
	{
		var area = this.FindChild<Grid>("Area");
		if (area != null)
		{
			area.Background = SnapTheme.OverlayBrush;
		}
	}

	public void OnHoverStyle()
	{
		var area = this.FindChild<Grid>("Area");
		if (area != null)
		{
			area.Background = SnapTheme.HighlightBrush;
		}
	}

	public Rectangle ScreenSnapArea(Dpi dpi)
	{
		var topLeft = this.PointToScreen(new Point(SnapControl.AreaPadding, SnapControl.AreaPadding));

		var bottomRight = this.PointToScreen(new Point(Bounds.Width - SnapControl.AreaPadding, Bounds.Height - SnapControl.AreaPadding));

		return new Rectangle(
		   topLeft.X,
		   topLeft.Y,
		   bottomRight.X,
		   bottomRight.Y,
		   dpi);
	}
}
