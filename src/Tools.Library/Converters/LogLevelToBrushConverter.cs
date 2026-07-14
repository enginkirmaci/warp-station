using Avalonia.Data.Converters;
using Avalonia.Media;
using Serilog.Events;

namespace Tools.Library.Converters;

/// <summary>
/// Maps a Serilog <see cref="LogEventLevel"/> to a text color for the log panel: errors/fatal
/// red, warnings amber, information muted, debug/verbose faint.
/// </summary>
public class LogLevelToBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is LogEventLevel level
            ? new SolidColorBrush(level switch
            {
                LogEventLevel.Fatal => Colors.OrangeRed,
                LogEventLevel.Error => Colors.OrangeRed,
                LogEventLevel.Warning => Colors.Goldenrod,
                LogEventLevel.Information => Colors.CornflowerBlue,
                LogEventLevel.Debug => Colors.Gray,
                LogEventLevel.Verbose => Colors.DimGray,
                _ => Colors.Gray,
            })
            : new SolidColorBrush(Colors.Gray);

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotImplementedException();
}
