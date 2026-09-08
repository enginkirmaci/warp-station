using Avalonia.Media;
using Avalonia.Media.Immutable;

namespace Tools.Library.Media;

/// <summary>
/// Cached pill/chip brushes shared by every chip converter — the single source of the
/// text and tint singletons (converters used to re-declare these per file, which let
/// their alphas drift apart). Text and glyph brushes use the bright hues (readable on
/// the dark theme); tints are the same hues at the standard 0.18 pill alpha.
/// </summary>
public static class ChipBrushes
{
    private const double TintAlpha = 0.18;

    public static readonly ImmutableSolidColorBrush GreenBright = new(Color.Parse(ChipPalette.GreenBright));
    public static readonly ImmutableSolidColorBrush AmberBright = new(Color.Parse(ChipPalette.AmberBright));
    public static readonly ImmutableSolidColorBrush PurpleBright = new(Color.Parse(ChipPalette.PurpleBright));
    public static readonly ImmutableSolidColorBrush BlueBright = new(Color.Parse(ChipPalette.BlueBright));
    public static readonly ImmutableSolidColorBrush Gray = new(Color.Parse(ChipPalette.Gray));
    public static readonly ImmutableSolidColorBrush RedStrong = new(Color.Parse(ChipPalette.RedStrong));
    public static readonly ImmutableSolidColorBrush RedDeleted = new(Color.Parse(ChipPalette.RedDeleted));

    public static readonly ImmutableSolidColorBrush GreenBrightTint = new(Color.Parse(ChipPalette.GreenBright), TintAlpha);
    public static readonly ImmutableSolidColorBrush AmberBrightTint = new(Color.Parse(ChipPalette.AmberBright), TintAlpha);
    public static readonly ImmutableSolidColorBrush PurpleBrightTint = new(Color.Parse(ChipPalette.PurpleBright), TintAlpha);
    public static readonly ImmutableSolidColorBrush BlueBrightTint = new(Color.Parse(ChipPalette.BlueBright), TintAlpha);
    public static readonly ImmutableSolidColorBrush GrayTint = new(Color.Parse(ChipPalette.GrayTint));
    public static readonly ImmutableSolidColorBrush RedTint = new(Color.Parse(ChipPalette.RedTint));
}
