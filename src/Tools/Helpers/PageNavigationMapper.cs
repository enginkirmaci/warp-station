using Tools.Views.Pages;

namespace Tools.Helpers;

/// <summary>
/// Maps between page types and navigation tags.
/// Implements Open/Closed Principle - extensible through modification of mappings.
/// </summary>
public static class PageNavigationMapper
{
    #region Private Fields

    private static readonly Type[] _registeredPageTypes =
    [
        typeof(DashboardPage),
        typeof(WorkspacesPage),
        typeof(NugetLocalPage),
        typeof(FormattersPage),
        typeof(ClipboardPasswordPage),
        typeof(EFToolsPage),
        typeof(CodeExecutePage),
        typeof(FocusTimerSettingsPage),
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