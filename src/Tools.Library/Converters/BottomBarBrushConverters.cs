using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Avalonia.Media.Immutable;
using Tools.Library.Media;

namespace Tools.Library.Converters;

/// <summary>
/// Colorizes a working-tree change's porcelain status code for the bottom bar's
/// changes lists: added green, deleted red, modified / renamed / type-changed warning
/// amber, untracked and anything else muted gray. The first status character decides
/// (two-character XY codes like "MM" color as their first letter).
/// <para>
/// The brushes are the shared cached singletons in <see cref="ChipBrushes"/>: this
/// converter runs per realized list row, so it must not allocate or hit the resource
/// tree.
/// </para>
/// </summary>
public class GitStatusBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return (value as string) switch
        {
            { Length: > 0 } code => code[0] switch
            {
                'A' => ChipBrushes.GreenBright,
                'D' => ChipBrushes.RedDeleted,
                'M' or 'R' or 'T' or 'C' => ChipBrushes.AmberBright,
                _ => ChipBrushes.Gray,
            },
            _ => ChipBrushes.Gray,
        };
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>
/// Colorizes a pull request's state chip text (<see cref="Entities.GitHubItem.StateChip"/>):
/// "Approved" green, "Review" purple, "Draft" and anything else muted gray.
/// <see cref="Convert"/> parameter "bg" returns a low-alpha TINT of the same color for
/// chip backgrounds (the full-strength brush is for the text). Cached brushes like
/// <see cref="GitStatusBrushConverter"/>.
/// </summary>
public class GitHubStateChipBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var tinted = string.Equals(parameter as string, "bg", StringComparison.OrdinalIgnoreCase);
        return value switch
        {
            "Approved" => tinted ? ChipBrushes.GreenBrightTint : ChipBrushes.GreenBright,
            "Review" => tinted ? ChipBrushes.PurpleBrightTint : ChipBrushes.PurpleBright,
            _ => tinted ? ChipBrushes.GrayTint : ChipBrushes.Gray,
        };
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}

/// <summary>
/// Colorizes a GitHub label pill by keyword: "bug" red, "enhancement" / "feature"
/// purple, "documentation" blue, "performance" amber, "good first issue" / "help
/// wanted" green — anything else muted gray (labels are free-form strings, so the
/// match is a case-insensitive Contains). <see cref="Convert"/> parameter "bg"
/// returns a low-alpha TINT of the same color for the pill background. Cached
/// brushes like <see cref="GitHubStateChipBrushConverter"/>.
/// </summary>
public class GitHubLabelBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var tinted = string.Equals(parameter as string, "bg", StringComparison.OrdinalIgnoreCase);
        return value as string switch
        {
            null => tinted ? ChipBrushes.GrayTint : ChipBrushes.Gray,
            { Length: > 0 } label => Contains(label, "bug", "crash", "regression") ? Tint(tinted, ChipBrushes.RedStrong, ChipBrushes.RedTint)
                : Contains(label, "enhancement", "feature", "improvement") ? Tint(tinted, ChipBrushes.PurpleBright, ChipBrushes.PurpleBrightTint)
                : Contains(label, "doc") ? Tint(tinted, ChipBrushes.BlueBright, ChipBrushes.BlueBrightTint)
                : Contains(label, "performance", "slow") ? Tint(tinted, ChipBrushes.AmberBright, ChipBrushes.AmberBrightTint)
                : Contains(label, "good first issue", "help wanted") ? Tint(tinted, ChipBrushes.GreenBright, ChipBrushes.GreenBrightTint)
                : Tint(tinted, ChipBrushes.Gray, ChipBrushes.GrayTint),
            _ => tinted ? ChipBrushes.GrayTint : ChipBrushes.Gray,
        };
    }

    private static bool Contains(string label, params string[] keywords)
    {
        foreach (var keyword in keywords)
        {
            if (label.Contains(keyword, StringComparison.OrdinalIgnoreCase)) return true;
        }

        return false;
    }

    private static ImmutableSolidColorBrush Tint(bool tinted, ImmutableSolidColorBrush text, ImmutableSolidColorBrush bg) => tinted ? bg : text;

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
