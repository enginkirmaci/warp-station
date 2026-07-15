using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Converters;

/// <summary>
/// Maps a <see cref="NotificationKind"/> to the toast accent stripe brush, resolving each
/// through the application's theme resources so colors track the active theme.
/// </summary>
public class NotificationKindToBrushConverter : IValueConverter
{
    private static readonly string[] ResourceKeys =
    {
        "InfoAccentBrush",      // Info
        "StatusActiveBrush",    // Success
        "WarningAccentBrush",   // Warning
        "DangerAccentBrush"     // Error
    };

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        if (value is NotificationKind kind)
        {
            var key = ResourceKeys[(int)kind];
            return Application.Current?.TryFindResource(key, out var brush) == true
                ? brush
                : Brushes.Transparent;
        }
        return Brushes.Transparent;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        throw new NotSupportedException();
}
