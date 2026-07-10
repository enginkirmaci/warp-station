using System.Windows.Input;
using Tools.Library.Entities;
using Tools.Library.Services;

namespace Tools.Library.Providers;

/// <summary>
/// Provides navigation item collections for the application.
/// Icon path data is resolved from .svg files in the Assets folder via <see cref="IconAssetLoader"/>.
/// </summary>
public static class NavigationProvider
{
    // Icon asset names (resolved to SVG path data by IconAssetLoader)
    private const string HomeIcon = "icon-home";
    private const string FolderIcon = "icon-folder-alt";
    private const string PackageIcon = "icon-package";
    private const string TextFormatIcon = "icon-text-format";
    private const string LockIcon = "icon-lock";
    private const string TerminalIcon = "icon-terminal-alt";
    private const string GridLayoutIcon = "icon-grid";

    // Page key for the Clipboard Password page, used to filter it from navigation
    // when its HideFromGui setting is enabled.
    private const string ClipboardPasswordPageKey = "ClipboardPasswordPage";

    private static string Icon(string name) => IconAssetLoader.GetPathData(name);

    /// <summary>
    /// Gets navigation items for dashboard cards.
    /// </summary>
    /// <param name="cardClickCommand">Command to execute when a card is clicked.</param>
    /// <param name="hideClipboardPassword">
    /// When <c>true</c>, omits the Clipboard Password card so the feature can be used
    /// via its hotkey without surfacing it in the dashboard.
    /// </param>
    /// <returns>Collection of navigation items.</returns>
    public static IReadOnlyCollection<NavigationItem> GetDashboardItems(
        ICommand? cardClickCommand,
        bool hideClipboardPassword = false)
    {
        var items = new List<NavigationItem>
        {
            CreateNavigationItem(
                "Repos",
                "Browse and open repositories",
                Icon(FolderIcon),
                "#FFFFFF",
                "ReposPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "NuGet Package Manager",
                "Copy new NuGet packages to destination",
                Icon(PackageIcon),
                "#FFFFFF",
                "NugetLocalPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Formatters",
                "Format and transform text",
                Icon(TextFormatIcon),
                "#FFFFFF",
                "FormattersPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Clipboard Password",
                "Generate and copy passwords",
                Icon(LockIcon),
                "#FFFFFF",
                ClipboardPasswordPageKey,
                cardClickCommand
            ),
            CreateNavigationItem(
                "Code Execute",
                "Run C# code snippets",
                Icon(TerminalIcon),
                "#FFFFFF",
                "CodeExecutePage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "SnapIt",
                "Window snapping and management",
                Icon(GridLayoutIcon),
                "#FFFFFF",
                "SnapItSettingsPage",
                cardClickCommand
            )
        };

        return hideClipboardPassword
            ? items.Where(i => i.PageKey != ClipboardPasswordPageKey).ToList()
            : items;
    }

    /// <summary>
    /// Gets navigation menu items for the sidebar navigation.
    /// </summary>
    /// <param name="hideClipboardPassword">
    /// When <c>true</c>, omits the Clipboard Password entry from the sidebar.
    /// </param>
    /// <returns>Collection of navigation items.</returns>
    public static IReadOnlyCollection<NavigationItem> GetNavigationMenuItems(bool hideClipboardPassword = false)
    {
        var items = new List<NavigationItem>
        {
            new NavigationItem
            {
                Title = "Dashboard",
                IconPath = Icon(HomeIcon),
                AccentColor = "#5B8DEF",
                PageKey = "DashboardPage"
            },
            // Separator marker
            new NavigationItem
            {
                Title = "---",
                PageKey = "__separator__"
            }
        };

        foreach (var item in GetDashboardItems(null, hideClipboardPassword))
        {
            items.Add(item);
        }

        return items;
    }

    private static NavigationItem CreateNavigationItem(
        string title,
        string subtitle,
        string iconPath,
        string accentColor,
        string pageKey,
        ICommand? command)
    {
        return new NavigationItem
        {
            Title = title,
            Subtitle = subtitle,
            Symbol = string.Empty,
            IconPath = iconPath,
            AccentColor = accentColor,
            PageKey = pageKey,
            Command = command!
        };
    }
}