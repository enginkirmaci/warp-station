using Avalonia;
using Avalonia.Media;

namespace Tools.Library.Media;

/// <summary>
/// Normalizes icon geometries so glyphs stay centered and uniformly scaled
/// inside the square Path elements used across the UI.
/// </summary>
public static class IconGeometry
{
    private const double DesignGridSize = 24;

    /// <summary>
    /// Centers the glyph within square bounds spanning the icon design grid.
    /// Avalonia's <c>Stretch.Uniform</c> aligns the scaled geometry to the
    /// top-left of the element instead of centering it, so icons whose path
    /// bounds are shorter than the element box render too high, and taller
    /// bounded icons render larger than the rest. The zero-area pad lines
    /// square off the bounds so every icon scales by the same design-grid
    /// ratio; being zero-area, they never contribute to the fill.
    /// </summary>
    public static Geometry CenterOnDesignGrid(Geometry icon)
    {
        var bounds = icon.Bounds;
        if (bounds.Width <= 0 || bounds.Height <= 0)
        {
            return icon;
        }

        var grid = Math.Max(DesignGridSize, Math.Max(bounds.Width, bounds.Height));
        icon.Transform = new TranslateTransform(
            grid / 2 - (bounds.X + bounds.Width / 2),
            grid / 2 - (bounds.Y + bounds.Height / 2));

        var framed = new GeometryGroup { FillRule = FillRule.NonZero };
        framed.Children.Add(icon);
        framed.Children.Add(new LineGeometry(new Point(0, 0), new Point(grid, 0)));
        framed.Children.Add(new LineGeometry(new Point(0, 0), new Point(0, grid)));
        return framed;
    }
}
