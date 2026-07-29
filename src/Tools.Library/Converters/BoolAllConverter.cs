using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

/// <summary>
/// Multi-value converter that returns <c>true</c> only when every bound
/// boolean input is <c>true</c>. Useful for combining a data-driven flag
/// (e.g. "is this item a separator?") with a global flag (e.g. "is the
/// sidebar expanded?") into a single <see cref="Avalonia.AvaloniaProperty"/>
/// such as <c>IsVisible</c>.
/// </summary>
public class BoolAllConverter : IMultiValueConverter
{
    public object? Convert(IList<object?> values, Type targetType, object? parameter, CultureInfo culture)
    {
        foreach (var value in values)
        {
            if (value is bool b)
            {
                if (!b)
                {
                    return false;
                }
            }
            else
            {
                return false;
            }
        }

        return true;
    }
}
