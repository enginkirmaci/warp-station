using Microsoft.UI.Xaml.Data;

namespace Tools.Library.Converters;

public class BooleanToStringConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        return (bool)value ? parameter.ToString()! : string.Empty;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        return null!;
    }
}