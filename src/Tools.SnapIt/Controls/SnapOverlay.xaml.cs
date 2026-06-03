using Avalonia.Controls;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Graphics;
using Point = Avalonia.Point;
using Size = Avalonia.Size;

namespace Tools.SnapIt.Controls;

public partial class SnapOverlay : UserControl
{
	public LayoutOverlay LayoutOverlay { get; internal set; }

	public SnapFullOverlay SnapFullOverlay { get; }

	public static readonly StyledProperty<SnapAreaTheme> SnapThemeProperty =
		AvaloniaProperty.Register<SnapOverlay, SnapAreaTheme>(
			nameof(SnapTheme),
			defaultBindingMode: BindingMode.TwoWay);

	public SnapAreaTheme SnapTheme
	{
		get => GetValue(SnapThemeProperty);
		set => SetValue(SnapThemeProperty, value);
	}

	static SnapOverlay()
	{
		SnapThemeProperty.Changed.AddClassHandler<SnapOverlay>((snapOverlayEditor, e) =>
		{
			if (snapOverlayEditor.SnapTheme != null)
			{
				snapOverlayEditor.Overlay.Opacity = snapOverlayEditor.SnapTheme.Opacity;
				snapOverlayEditor.Overlay.Background = snapOverlayEditor.SnapTheme.OverlayBrush;
				snapOverlayEditor.Border.BorderBrush = snapOverlayEditor.SnapTheme.BorderBrush;
				snapOverlayEditor.Border.BorderThickness = new Thickness(snapOverlayEditor.SnapTheme.BorderThickness);
			}
		});
	}

	public SnapOverlay(SnapAreaTheme theme, SnapFullOverlay snapFullOverlay)
	{
		InitializeComponent();
		DataContext = this;

		Name = $"snapoverlay_{Guid.NewGuid():N}";

		SnapTheme = theme;
		SnapFullOverlay = snapFullOverlay;

		SizeChanged += SnapOverlay_SizeChanged;
	}

	public void SetPos(Control element, Point point, Size size)
	{
		element.Margin = new Thickness(point.X, point.Y, 0, 0);

		element.Width = size.Width;
		element.Height = size.Height;
	}

	public void SetPos(LayoutOverlay layoutOverlay)
	{
		if (layoutOverlay != null)
		{
			SetPos(this, layoutOverlay.Point.Convert(), layoutOverlay.Size.Convert());
		}
		else
		{
			var factor = 0.3;
			Width = SnapFullOverlay.Width * factor;
			Height = SnapFullOverlay.Height * factor;

			Margin = new Thickness(
			   SnapFullOverlay.Margin.Left + (SnapFullOverlay.Width / 2 - Width / 2),
			   SnapFullOverlay.Margin.Top + (SnapFullOverlay.Height / 2 - Height / 2),
				0, 0);
		}
	}

	public void NormalStyle()
	{
		Overlay.Background = SnapTheme.OverlayBrush;
		Border.IsVisible = true;
		MergedIcon.IsVisible = true;

		SnapFullOverlay.NormalStyle();
	}

	public void OnHoverStyle()
	{
		Overlay.Background = Brushes.Transparent;
		Border.IsVisible = false;
		MergedIcon.IsVisible = false;

		SnapFullOverlay.OnHoverStyle();
	}

	private void SnapOverlay_SizeChanged(object sender, SizeChangedEventArgs e)
	{
		var iconFactor = 0.15;
		if (Bounds.Width > Bounds.Height)
		{
			var size = MergedIcon.Width = MergedIcon.Height = Bounds.Width * iconFactor;
			icon1.FontSize = size > 0 ? size : 1;
			icon2.FontSize = size > 0 ? size : 1;
		}
		else
		{
			var size = MergedIcon.Width = MergedIcon.Height = Bounds.Height * iconFactor;
			icon1.FontSize = size > 0 ? size : 1;
			icon2.FontSize = size > 0 ? size : 1;
		}
	}

	public Rectangle ScreenSnapArea(Dpi dpi)
	{
		return SnapFullOverlay.ScreenSnapArea(dpi);
	}
}
