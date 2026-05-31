using Avalonia;
using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

public class EqualBooleanConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value?.ToString() == parameter?.ToString();
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value is true ? parameter : AvaloniaProperty.UnsetValue;
    }
}