using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media.Immutable;
using Tools.Library.Entities;
using Tools.Library.Media;

namespace Tools.Library.Converters;

/// <summary>
/// Colorizes a repo tag name for the bottom-bar header's tag chips: the reserved
/// <c>favorites</c> tag takes the star's amber, the scanner's <c>platform</c> tag
/// blue, and every other name one of the four bright hues by a stable per-name hash.
/// The hash is FNV-1a over the lower-cased name — deterministic across runs and
/// processes, unlike <c>string.GetHashCode</c> which is randomized.
/// <para>
/// With no converter parameter the solid bright is returned (use for the chip's
/// text); with <c>ConverterParameter=bg</c> the same hue's translucent tint is
/// returned (use for the chip background). The brushes are the shared cached
/// singletons in <see cref="ChipBrushes"/> — never local brush fields.
/// </para>
/// </summary>
public class TagChipBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var tinted = parameter is "bg";
        return value switch
        {
            string name => Pick(name, tinted),
            _ => tinted ? ChipBrushes.GrayTint : ChipBrushes.Gray,
        };
    }

    private static ImmutableSolidColorBrush Pick(string name, bool tinted)
    {
        if (name.Trim().Equals(Repo.FavoritesTag, StringComparison.OrdinalIgnoreCase))
        {
            return tinted ? ChipBrushes.AmberBrightTint : ChipBrushes.AmberBright;
        }

        if (name.Trim().Equals(Repo.PlatformTag, StringComparison.OrdinalIgnoreCase))
        {
            return tinted ? ChipBrushes.BlueBrightTint : ChipBrushes.BlueBright;
        }

        unchecked
        {
            uint hash = 2166136261;
            foreach (var c in name.Trim().ToLowerInvariant())
            {
                hash ^= c;
                hash *= 16777619;
            }

            return (hash % 4) switch
            {
                0 => tinted ? ChipBrushes.GreenBrightTint : ChipBrushes.GreenBright,
                1 => tinted ? ChipBrushes.BlueBrightTint : ChipBrushes.BlueBright,
                2 => tinted ? ChipBrushes.PurpleBrightTint : ChipBrushes.PurpleBright,
                _ => tinted ? ChipBrushes.AmberBrightTint : ChipBrushes.AmberBright,
            };
        }
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
