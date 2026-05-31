using Avalonia;
using Avalonia.Data.Converters;
using Avalonia.Media;

namespace Tools.Library.Converters;

/// <summary>
/// Converts an SVG path data string to an Avalonia Geometry object.
/// Falls back to a simple filled circle if parsing fails.
/// </summary>
public class StringToGeometryConverter : IValueConverter
{
    // Simple 24x24 circle as fallback
    private static Geometry? _fallbackGeometry;

    private static Geometry FallbackGeometry
    {
        get
        {
            if (_fallbackGeometry == null)
            {
                try
                {
                    _fallbackGeometry = Geometry.Parse("M12,2 A10,10 0 1,1 12,22 A10,10 0 1,1 12,2 Z");
                }
                catch
                {
                    _fallbackGeometry = new RectangleGeometry(new Rect(0, 0, 24, 24));
                }
            }
            return _fallbackGeometry;
        }
    }

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string pathData && !string.IsNullOrWhiteSpace(pathData))
        {
            try
            {
                // Replace commas with spaces for consistent Avalonia path parser behavior
                var normalized = pathData.Replace(",", " ");
                var geometry = Geometry.Parse(normalized);
                // Verify the geometry is non-empty
                if (geometry.Bounds.Width > 0 || geometry.Bounds.Height > 0)
                {
                    return geometry;
                }
            }
            catch
            {
                // Path parsing failed, fall through to fallback
            }
        }

        return FallbackGeometry;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
