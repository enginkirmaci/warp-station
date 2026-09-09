using Avalonia.Data.Converters;
using Avalonia.Media;
using Tools.Library.Media;

namespace Tools.Library.Converters;

/// <summary>
/// Maps a bool to a brush pair: true → <see cref="TrueBrush"/>, false →
/// <see cref="FalseBrush"/>, null → <see cref="FalseBrush"/> too (a null path hop
/// reads as "not set"). The defaults are the shared status-dot colors (on/success
/// green, failed red, neutral gray) from <see cref="ChipBrushes"/>; the favorite star
/// reuses this converter with its gold/muted pair via App.xaml's parameterized
/// resource. Brushes are cached singletons — this converter is evaluated per visible
/// row, so it must not allocate.
/// </summary>
public class BoolToBrushConverter : IValueConverter
{
    /// <summary>Brush for <see langword="true"/>; defaults to the shared success green.</summary>
    public IBrush TrueBrush { get; set; } = ChipBrushes.GreenBright;

    /// <summary>Brush for <see langword="false"/>; defaults to the shared failure red.</summary>
    public IBrush FalseBrush { get; set; } = ChipBrushes.RedStrong;

    /// <summary>Brush for a null binding value (a path hop through a null object);
    /// defaults to the shared neutral gray.</summary>
    public IBrush NullBrush { get; set; } = ChipBrushes.Gray;

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
