using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Avalonia.Media.Immutable;
using Tools.Library.Media;

namespace Tools.Library.Converters;

/// <summary>
/// Colorizes a git branch name for the Repos table's branch pill, by branch
/// family: <c>main</c> green, <c>develop</c>/<c>development</c> blue,
/// <c>release</c> orange, <c>master</c> purple, everything else muted gray.
/// The family is matched on the branch name's first path segment
/// (<c>release/1.2</c> colors as <c>release</c>), case-insensitively.
/// <para>
/// With no converter parameter the solid accent is returned (use for the text
/// and glyph); with <c>ConverterParameter=bg</c> the same color's translucent
/// tint is returned (use for the pill background), matching the 0.16 opacity
/// the other Repos chips use.
/// </para>
/// <para>
/// The brushes are the shared cached singletons in <see cref="ChipBrushes"/> (hex
/// colors from <see cref="ChipPalette"/>): this converter runs for every realized
/// repo row, so it must not allocate — the family match slices the branch name in
/// place and compares spans ordinally case-insensitively, with no intermediate
/// strings.
/// </para>
/// </summary>
public class BranchChipBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var tinted = parameter is "bg";
        return value switch
        {
            string name => MatchHead(name.AsSpan(), tinted),
            _ => tinted ? ChipBrushes.GrayTint : ChipBrushes.Gray,
        };
    }

    /// <summary>
    /// Matches the branch family of <paramref name="name"/> — its first
    /// '/'-segment, whitespace-trimmed — against the known family literals,
    /// case-insensitively. Allocation-free equivalent of the previous
    /// <c>Trim().Split('/', 2)[0].ToLowerInvariant()</c> pipeline.
    /// </summary>
    private static ImmutableSolidColorBrush MatchHead(ReadOnlySpan<char> name, bool tinted)
    {
        var head = name.Trim();
        var separator = head.IndexOf('/');
        if (separator >= 0)
        {
            head = head[..separator];
        }

        if (head.Equals("main", StringComparison.OrdinalIgnoreCase))
        {
            return tinted ? ChipBrushes.GreenBrightTint : ChipBrushes.GreenBright;
        }

        if (head.Equals("develop", StringComparison.OrdinalIgnoreCase)
            || head.Equals("development", StringComparison.OrdinalIgnoreCase))
        {
            return tinted ? ChipBrushes.BlueBrightTint : ChipBrushes.BlueBright;
        }

        if (head.Equals("release", StringComparison.OrdinalIgnoreCase))
        {
            return tinted ? ChipBrushes.AmberBrightTint : ChipBrushes.AmberBright;
        }

        if (head.Equals("master", StringComparison.OrdinalIgnoreCase))
        {
            return tinted ? ChipBrushes.PurpleBrightTint : ChipBrushes.PurpleBright;
        }

        return tinted ? ChipBrushes.GrayTint : ChipBrushes.Gray;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
