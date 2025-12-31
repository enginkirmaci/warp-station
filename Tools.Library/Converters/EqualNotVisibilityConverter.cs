using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;

namespace Tools.Library.Converters;

public class EqualNotVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        if (value != null && value.Equals(parameter))
        {
            return Visibility.Collapsed;
        }

        return Visibility.Visible;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return null!;
    }
}