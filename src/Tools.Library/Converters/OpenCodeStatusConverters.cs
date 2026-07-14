using Avalonia.Data.Converters;
using Avalonia.Media;
using Tools.Library.Entities;

namespace Tools.Library.Converters;

/// <summary>
/// Maps an <see cref="OpenCodeInstanceStatus"/> to the status-dot color used on repo cards:
/// Running (green), Starting (amber), Error (red), Stopped (muted gray).
/// </summary>
public class OpenCodeStatusToBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is OpenCodeInstanceStatus status
            ? new SolidColorBrush(status switch
            {
                OpenCodeInstanceStatus.Running => Colors.LimeGreen,
                OpenCodeInstanceStatus.Starting => Colors.Goldenrod,
                OpenCodeInstanceStatus.Error => Colors.OrangeRed,
                _ => Colors.Gray,
            })
            : new SolidColorBrush(Colors.Gray);

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotImplementedException();
}

/// <summary>
/// Maps an <see cref="OpenCodeInstanceStatus"/> to its display label for the repo card.
/// </summary>
public class OpenCodeStatusToTextConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is OpenCodeInstanceStatus status
            ? status switch
            {
                OpenCodeInstanceStatus.Running => "Running",
                OpenCodeInstanceStatus.Starting => "Starting",
                OpenCodeInstanceStatus.Error => "Error",
                _ => "Stopped",
            }
            : "Stopped";

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotImplementedException();
}
