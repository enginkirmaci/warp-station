using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

public class EqualNotVisibilityConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value != null && value.Equals(parameter))
        {
            return false;
        }

        return true;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return null;
    }
}