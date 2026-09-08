using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

/// <summary>
/// Returns <c>true</c> only when the bound value is a positive number: counts of zero
/// and non-numeric values (including null, which is what a null binding path yields)
/// are treated as "not available" and yield <c>false</c>. The bool return feeds
/// <c>IsVisible</c> directly — used for the Pull/Push buttons' ahead/behind counts,
/// which should only surface when git actually reports commits.
/// </summary>
public class PositiveCountToVisibilityConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value is int count && count > 0
            || value is long count64 && count64 > 0;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return null;
    }
}
