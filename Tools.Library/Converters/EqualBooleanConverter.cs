using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;

namespace Tools.Library.Converters;

public class EqualBooleanConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        return value.ToString() == parameter.ToString();
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return (bool)value ? parameter : DependencyProperty.UnsetValue;
    }
}