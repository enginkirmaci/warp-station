using Tools.Views.Pages;

namespace Tools.Helpers;

/// <summary>
/// Single source of truth for registered application pages and the mapping between
/// page keys (strings) and page <see cref="Type"/>s. Consolidates the former
/// <c>NameToPageTypeConverter</c> reflection scan into one explicit registry so the
/// two mechanisms cannot drift out of sync.
/// </summary>
public static class PageNavigationMapper
{
    #region Private Fields

    private static readonly Type[] _registeredPageTypes =
    [
        typeof(DashboardPage),
        typeof(ReposPage),
        typeof(NugetLocalPage),
        typeof(FormattersPage),
        typeof(ClipboardPasswordPage),
        typeof(EFToolsPage),
        typeof(CodeExecutePage),
        typeof(SnapItSettingsPage)
    ];

    #endregion Private Fields

    #region Public Methods

    /// <summary>
    /// Maps a page type to its navigation tag.
    /// </summary>
    /// <param name="pageType">The page type to map.</param>
    /// <returns>The navigation tag, or null if not found.</returns>
    public static string? GetTagFromPageType(Type pageType)
    {
        return IsRegistered(pageType) ? pageType.Name : null;
    }

    /// <summary>
    /// Maps a navigation tag to its page type.
    /// </summary>
    /// <param name="tag">The navigation tag to map.</param>
    /// <returns>The page type, or null if not found.</returns>
    public static Type? GetPageTypeFromTag(string? tag)
    {
        if (string.IsNullOrEmpty(tag))
            return null;
        return Array.Find(_registeredPageTypes, t => t.Name == tag);
    }

    /// <summary>
    /// Converts a page name to its <see cref="Type"/>.
    /// </summary>
    /// <param name="pageName">
    /// The page name. Matched either as the exact type name or with a "Page" suffix appended.
    /// </param>
    /// <returns>The matching page type, or <c>null</c> if none is registered.</returns>
    public static Type? Convert(string? pageName)
    {
        if (string.IsNullOrWhiteSpace(pageName))
            return null;

        // Exact match against registered page type names (e.g. "DashboardPage").
        var exact = GetPageTypeFromTag(pageName);
        if (exact != null)
            return exact;

        // Allow keys without the "Page" suffix (e.g. "Dashboard" -> "DashboardPage").
        return GetPageTypeFromTag(pageName + "Page");
    }

    /// <summary>
    /// Checks if a page type is registered.
    /// </summary>
    /// <param name="pageType">The page type to check.</param>
    /// <returns>True if the page type is registered, otherwise false.</returns>
    public static bool IsRegistered(Type pageType)
    {
        return Array.Exists(_registeredPageTypes, t => t == pageType);
    }

    /// <summary>
    /// Checks if a tag is registered.
    /// </summary>
    /// <param name="tag">The tag to check.</param>
    /// <returns>True if the tag is registered, otherwise false.</returns>
    public static bool IsRegistered(string? tag)
    {
        if (string.IsNullOrEmpty(tag))
            return false;
        return Array.Exists(_registeredPageTypes, t => t.Name == tag);
    }

    #endregion Public Methods
}
