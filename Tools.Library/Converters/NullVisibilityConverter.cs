using System.Collections;

namespace Tools.Library.Converters;

public class NullVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, CultureInfo culture)
    {
        // Hide if null or (if collection) empty
        if (value == null)
            return Visibility.Collapsed;

        if (value is ICollection collection && collection.Count == 0)
            return Visibility.Collapsed;

        if (parameter != null && (bool)parameter == true)
        {
            return value != null ? Visibility.Collapsed : Visibility.Visible;
        }

        return Visibility.Visible;
    }

    public object ConvertBack(object value, Type targetType, object parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}