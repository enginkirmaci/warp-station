using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

/// <summary>
/// Returns <c>true</c> when the bound value is usable text, otherwise <c>false</c>:
/// <c>null</c> and the empty string yield <c>false</c>; any non-null non-string
/// value and any non-empty string (including whitespace-only) yield <c>true</c>.
/// <para>
/// Deliberately NOT the same contract as <see cref="StringNotNullToBoolConverter"/>
/// (which also rejects whitespace and non-strings) or
/// <see cref="NotNullToBoolConverter"/> (which accepts anything non-null) — all
/// three stay in use, so do not merge them. The bool return feeds
/// <c>IsVisible</c> directly.
/// </para>
/// </summary>
public class StringNullOrEmptyToVisibilityConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value == null)
        {
            return false;
        }

        if (value is string s && string.IsNullOrEmpty(s))
        {
            return false;
        }

        return true;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return null;
    }
}