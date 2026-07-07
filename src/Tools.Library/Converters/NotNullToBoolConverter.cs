using System.Globalization;
using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

/// <summary>
/// Returns <c>true</c> when the bound value is non-null, otherwise <c>false</c>.
/// </summary>
public class NotNullToBoolConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is not null;

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
