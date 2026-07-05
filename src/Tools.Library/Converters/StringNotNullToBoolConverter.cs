using System.Globalization;
using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

/// <summary>
/// Returns <c>true</c> when the bound string is non-null and non-empty, otherwise
/// <c>false</c>. Used to enable/disable actions that require a value (e.g. the Visual
/// Studio button is only enabled when a repo has a solution path).
/// </summary>
public class StringNotNullToBoolConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => !string.IsNullOrWhiteSpace(value as string);

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
