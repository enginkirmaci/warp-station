using Microsoft.UI;
using Microsoft.UI.Xaml.Data;

namespace Tools.Library.Converters;

public class BoolToColorConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        if (value is bool isTrue)
        {
            return isTrue ? Colors.LimeGreen : Colors.OrangeRed;
        }

        return Colors.Gray;
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        throw new NotImplementedException();
    }
}
