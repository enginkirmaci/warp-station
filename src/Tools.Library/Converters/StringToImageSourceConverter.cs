using Avalonia.Data.Converters;
using Avalonia.Media.Imaging;

namespace Tools.Library.Converters;

public class StringToImageSourceConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is not string path)
        {
            return null;
        }

        if (string.IsNullOrEmpty(path))
        {
            return null;
        }

        return new Bitmap(path);
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return null;
    }
}