using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

public class BooleanToStringConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value is true ? parameter?.ToString() ?? string.Empty : string.Empty;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return null;
    }
}