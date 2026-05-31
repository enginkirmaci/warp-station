using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

public class UriConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is string s && Uri.TryCreate(s, UriKind.RelativeOrAbsolute, out Uri? uri))
        {
            return uri;
        }

        return null;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is Uri u)
        {
            return u.OriginalString;
        }

        return null;
    }
}