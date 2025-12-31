using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media.Imaging;

namespace Tools.Library.Converters;

public class StringToImageSourceConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        if (value is not string path)
        {
            return null!;
        }

        if (string.IsNullOrEmpty(path))
        {
            return null!;
        }

        return new BitmapImage(new Uri(path, UriKind.RelativeOrAbsolute));
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return null!;
    }
}