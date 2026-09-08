using Avalonia.Data.Converters;
using Avalonia.Media;
using Avalonia.Media.Immutable;
using Tools.Library.Media;

namespace Tools.Library.Converters;

/// <summary>
/// Maps a bool to the shared status dot fill: true = on/success green, false =
/// failed red, null = neutral gray. The brushes are cached singletons built from
/// <see cref="ChipPalette"/> (like the other chip converters) — this converter is
/// evaluated per visible row, so allocating per call would churn the heap.
/// </summary>
public class BoolToBrushConverter : IValueConverter
{
    private static readonly ImmutableSolidColorBrush TrueBrush = new(Color.Parse(ChipPalette.GreenStrong));

    private static readonly ImmutableSolidColorBrush FalseBrush = new(Color.Parse(ChipPalette.RedStrong));

    private static readonly ImmutableSolidColorBrush NullBrush = new(Color.Parse(ChipPalette.Gray));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return value switch
        {
            true => TrueBrush,
            false => FalseBrush,
            _ => NullBrush,
        };
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        throw new NotImplementedException();
    }
}
