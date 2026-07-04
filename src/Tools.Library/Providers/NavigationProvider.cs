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
    private const string DatabaseIcon = "icon-database";
    private const string TerminalIcon = "icon-terminal-alt";
    private const string GridLayoutIcon = "icon-grid";

    private static string Icon(string name) => IconAssetLoader.GetPathData(name);

    /// <summary>
    /// Gets navigation items for dashboard cards.
    /// </summary>
    /// <param name="cardClickCommand">Command to execute when a card is clicked.</param>
    /// <returns>Collection of navigation items.</returns>
    public static IReadOnlyCollection<NavigationItem> GetDashboardItems(ICommand? cardClickCommand)
    {
        return new List<NavigationItem>
        {
            CreateNavigationItem(
                "Workspaces",
                "Manage your workspaces",
                Icon(FolderIcon),
                "#FFFFFF",
                "WorkspacesPage",
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
                "ClipboardPasswordPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "EF Tools",
                "Entity Framework tools and utilities",
                Icon(DatabaseIcon),
                "#FFFFFF",
                "EFToolsPage",
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
    }

    /// <summary>
    /// Gets navigation menu items for the sidebar navigation.
    /// </summary>
    /// <returns>Collection of navigation items.</returns>
    public static IReadOnlyCollection<NavigationItem> GetNavigationMenuItems()
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

        foreach (var item in GetDashboardItems(null))
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