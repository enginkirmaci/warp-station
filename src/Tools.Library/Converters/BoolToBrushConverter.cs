using Avalonia.Data.Converters;
using Avalonia.Media;

namespace Tools.Library.Converters;

public class BoolToBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is bool isTrue)
        {
            return new SolidColorBrush(isTrue ? Colors.LimeGreen : Colors.OrangeRed);
        }

        return new SolidColorBrush(Colors.Gray);
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
