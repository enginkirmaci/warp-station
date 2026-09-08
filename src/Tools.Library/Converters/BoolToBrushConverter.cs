using Avalonia.Data.Converters;
using Tools.Library.Media;

namespace Tools.Library.Converters;

/// <summary>
/// Maps a bool to the shared status dot fill: true = on/success green, false =
/// failed red, null = neutral gray. The brushes are the shared cached singletons
/// in <see cref="ChipBrushes"/> — this converter is evaluated per visible row,
/// so it must not allocate.
/// </summary>
public class BoolToBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value switch
        {
            true => ChipBrushes.GreenBright,
            false => ChipBrushes.RedStrong,
            _ => ChipBrushes.Gray,
        };
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
