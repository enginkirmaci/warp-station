using Avalonia.Data.Converters;
using System.Collections;

namespace Tools.Library.Converters;

public class CountToVisibilityConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value == null)
        {
            return false;
        }

        long count = 0;

        switch (value)
        {
            case int i:
                count = i;
                break;
            case long l:
                count = l;
                break;
            case short s:
                count = s;
                break;
            case uint ui:
                count = ui;
                break;
            case ulong ul:
                count = (long)ul;
                break;
            case string str when long.TryParse(str, out var parsed):
                count = parsed;
                break;
            case ICollection collection:
                count = collection.Count;
                break;
        }

        return count > 0;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return null;
    }
}
