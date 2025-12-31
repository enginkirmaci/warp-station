using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;

namespace Tools.Library.Converters;

public class NullVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        return value == null ? Visibility.Collapsed : Visibility.Visible;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return null!;
    }
}