using System.Collections.Generic;
using System.Globalization;
using Avalonia.Data.Converters;

namespace Tools.Library.Converters;

/// <summary>
/// Formats the repos-table Changes cell's three counts (modified / to push / to pull)
/// into one short tooltip: "12 modified · 2 to push · 1 to pull". Zero entries drop
/// out; all-zero falls back to the cell's plain action text.
/// </summary>
public class GitCountsTooltipConverter : IMultiValueConverter
{
    public object? Convert(IList<object?> values, Type targetType, object? parameter, CultureInfo culture)
    {
        int Count(int i) => values.Count > i && values[i] is int n && n > 0 ? n : 0;

        var modified = Count(0);
        var push = Count(1);
        var pull = Count(2);
        if (modified == 0 && push == 0 && pull == 0) return "Open the changes panel";

        var parts = new List<string>(3);
        if (modified > 0) parts.Add($"{modified} modified");
        if (push > 0) parts.Add($"{push} to push");
        if (pull > 0) parts.Add($"{pull} to pull");
        return string.Join(" · ", parts);
    }
}
