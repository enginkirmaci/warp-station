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
/// The brushes are cached singletons with the theme palette's own colors (matching
/// <see cref="BranchChipBrushConverter"/>'s approach): this converter runs per realized
/// list row, so it must not allocate or hit the resource tree.
/// </para>
/// </summary>
public class GitStatusBrushConverter : IValueConverter
{
    private static readonly ImmutableSolidColorBrush Added = new(Color.Parse(ChipPalette.GreenStrong));
    private static readonly ImmutableSolidColorBrush Deleted = new(Color.Parse(ChipPalette.RedDeleted));
    private static readonly ImmutableSolidColorBrush Modified = new(Color.Parse(ChipPalette.AmberStrong));
    private static readonly ImmutableSolidColorBrush Other = new(Color.Parse(ChipPalette.Gray));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        return (value as string) switch
        {
            { Length: > 0 } code => code[0] switch
            {
                'A' => Added,
                'D' => Deleted,
                'M' or 'R' or 'T' or 'C' => Modified,
                _ => Other,
            },
            _ => Other,
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
    private static readonly ImmutableSolidColorBrush Approved = new(Color.Parse(ChipPalette.GreenStrong));
    private static readonly ImmutableSolidColorBrush Review = new(Color.Parse(ChipPalette.Purple));
    private static readonly ImmutableSolidColorBrush Draft = new(Color.Parse(ChipPalette.Gray));
    private static readonly ImmutableSolidColorBrush ApprovedBg = new(Color.Parse(ChipPalette.GreenTint));
    private static readonly ImmutableSolidColorBrush ReviewBg = new(Color.Parse(ChipPalette.PurpleTint));
    private static readonly ImmutableSolidColorBrush DraftBg = new(Color.Parse(ChipPalette.GrayTint));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var tinted = string.Equals(parameter as string, "bg", StringComparison.OrdinalIgnoreCase);
        return value switch
        {
            "Approved" => tinted ? ApprovedBg : Approved,
            "Review" => tinted ? ReviewBg : Review,
            _ => tinted ? DraftBg : Draft,
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
    private static readonly ImmutableSolidColorBrush Bug = new(Color.Parse(ChipPalette.RedStrong));
    private static readonly ImmutableSolidColorBrush Enhancement = new(Color.Parse(ChipPalette.PurpleStrong));
    private static readonly ImmutableSolidColorBrush Documentation = new(Color.Parse(ChipPalette.BlueStrong));
    private static readonly ImmutableSolidColorBrush Performance = new(Color.Parse(ChipPalette.AmberStrong));
    private static readonly ImmutableSolidColorBrush Community = new(Color.Parse(ChipPalette.GreenStrong));
    private static readonly ImmutableSolidColorBrush Other = new(Color.Parse(ChipPalette.Gray));
    private static readonly ImmutableSolidColorBrush BugBg = new(Color.Parse(ChipPalette.RedTint));
    private static readonly ImmutableSolidColorBrush EnhancementBg = new(Color.Parse(ChipPalette.PurpleTint));
    private static readonly ImmutableSolidColorBrush DocumentationBg = new(Color.Parse(ChipPalette.BlueTint));
    private static readonly ImmutableSolidColorBrush PerformanceBg = new(Color.Parse(ChipPalette.AmberTint));
    private static readonly ImmutableSolidColorBrush CommunityBg = new(Color.Parse(ChipPalette.GreenTint));
    private static readonly ImmutableSolidColorBrush OtherBg = new(Color.Parse(ChipPalette.GrayTint));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var tinted = string.Equals(parameter as string, "bg", StringComparison.OrdinalIgnoreCase);
        return value as string switch
        {
            null => tinted ? OtherBg : Other,
            { Length: > 0 } label => Contains(label, "bug", "crash", "regression") ? Tint(tinted, Bug, BugBg)
                : Contains(label, "enhancement", "feature", "improvement") ? Tint(tinted, Enhancement, EnhancementBg)
                : Contains(label, "doc") ? Tint(tinted, Documentation, DocumentationBg)
                : Contains(label, "performance", "slow") ? Tint(tinted, Performance, PerformanceBg)
                : Contains(label, "good first issue", "help wanted") ? Tint(tinted, Community, CommunityBg)
                : Tint(tinted, Other, OtherBg),
            _ => tinted ? OtherBg : Other,
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
