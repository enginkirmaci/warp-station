using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Graphics;
using Point = Avalonia.Point;
using Size = Avalonia.Size;

namespace Tools.SnapIt.Controls;

public class SnapFullOverlay : TemplatedControl
{
    public LayoutOverlay LayoutOverlay { get; internal set; }

    public static readonly StyledProperty<SnapAreaTheme> SnapThemeProperty =
        AvaloniaProperty.Register<SnapFullOverlay, SnapAreaTheme>(
            nameof(SnapTheme),
            defaultValue: new SnapAreaTheme(),
            defaultBindingMode: BindingMode.TwoWay);

    public SnapAreaTheme SnapTheme
    {
        get => GetValue(SnapThemeProperty);
        set => SetValue(SnapThemeProperty, value);
    }

    static SnapFullOverlay()
    {
        SnapThemeProperty.Changed.AddClassHandler<SnapFullOverlay>((snapOverlayEditor, e) =>
        {
            snapOverlayEditor.SnapTheme = e.NewValue as SnapAreaTheme;
        });
    }

    public SnapFullOverlay(SnapAreaTheme theme)
    {
        SnapTheme = theme;
    }

    public void SetPos(Point point, Size size)
    {
        Margin = new Thickness(point.X, point.Y, 0, 0);

        Width = size.Width;
        Height = size.Height;
    }

    public void NormalStyle()
    {
        IsVisible = false;
    }

    public void OnHoverStyle()
    {
        IsVisible = true;
        ApplyTemplate();

        var overlay = this.FindChild<Grid>("Overlay");
        if (overlay != null)
        {
            overlay.Opacity = SnapTheme.Opacity;
        }
    }

    public Rectangle ScreenSnapArea(Dpi dpi)
    {
        var topLeft = this.PointToScreen(new Point(0, 0));

        var bottomRight = this.PointToScreen(new Point(Bounds.Width, Bounds.Height));

        return new Rectangle(
           topLeft.X,
           topLeft.Y,
           bottomRight.X,
           bottomRight.Y,
           dpi);
    }
}
