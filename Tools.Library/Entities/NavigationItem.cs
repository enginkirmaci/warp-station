using Tools.Library.Models;

namespace Tools.Library.Entities;

/// <summary>
/// Deprecated: Use Tools.Library.Models.NavigationItem instead.
/// This class is kept for backward compatibility.
/// </summary>
[Obsolete("Use Tools.Library.Models.NavigationItem instead.")]
public class NavigationItem : Tools.Library.Models.NavigationItem
{
    /// <summary>
    /// Deprecated: Use PageKey instead.
    /// </summary>
    [Obsolete("Use PageKey instead.")]
    public object? CommandParameter
    {
        get => PageKey;
        set => PageKey = value?.ToString() ?? string.Empty;
    }

    /// <summary>
    /// Deprecated: No longer used.
    /// </summary>
    [Obsolete("No longer used.")]
    public Type? TargetPageType { get; set; }
}