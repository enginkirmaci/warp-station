using System.Windows.Input;
using Tools.Library.Entities;

namespace Tools.Library.Providers;

/// <summary>
/// Provides navigation item collections for the application.
/// </summary>
public static class NavigationProvider
{
    // Icon path data (24x24 viewbox SVG paths)
    private const string HomeIcon = "M12,3 L2,12 L5,12 L5,21 L10,21 L10,15 L14,15 L14,21 L19,21 L19,12 L22,12 Z";
    private const string FolderIcon = "M2,6 L10,6 L12,4 L22,4 L22,20 L2,20 Z";
    private const string PackageIcon = "M21,8 L12,3 L3,8 L3,20 L21,20 Z M12,3 L12,20 M3,12 L21,12";
    private const string TextFormatIcon = "M3,4 L17,4 L17,6 L12,6 L12,8 L16,8 L16,10 L12,10 L12,18 L14,18 L14,20 L8,20 L8,18 L10,18 L10,10 L5,10 L5,8 L10,8 L10,6 L3,6 Z";
    private const string LockIcon = "M17,10 L17,8 C17,4.7 14.8,2 12,2 C9.2,2 7,4.7 7,8 L7,10 L5,10 L5,21 L19,21 L19,10 Z M9,8 C9,5.8 10.3,4 12,4 C13.7,4 15,5.8 15,8 L15,10 L9,10 Z";
    private const string DatabaseIcon = "M4,5 L20,5 C20,5 20,8 20,9.5 C20,11 20,12 20,12 L4,12 C4,12 4,11 4,9.5 C4,8 4,5 4,5 Z M4,12 C4,12 4,13 4,14.5 C4,16 4,17 4,17 L20,17 C20,17 20,16 20,14.5 C20,13 20,12 20,12 Z M4,17 C4,17 4,18 4,19.5 C4,20 4,20 4,20 L20,20 C20,20 20,20 20,19.5 C20,18 20,17 20,17 Z";
    private const string TerminalIcon = "M2,4 L22,4 L22,20 L2,20 Z M5,8 L9,12 L5,16 Z M11,16 L19,16";
    private const string ClockIcon = "M12,2 C6.5,2 2,6.5 2,12 C2,17.5 6.5,22 12,22 C17.5,22 22,17.5 22,12 C22,6.5 17.5,2 12,2 Z M12,6 L12,12 L16,14";
    private const string GridLayoutIcon = "M3,3 L10,3 L10,10 L3,10 Z M13,3 L21,3 L21,10 L13,10 Z M3,13 L10,13 L10,21 L3,21 Z M13,13 L21,13 L21,21 L13,21 Z";

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
                FolderIcon,
                "#4CAF50",
                "WorkspacesPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "NuGet Package Manager",
                "Copy new NuGet packages to destination",
                PackageIcon,
                "#FF9800",
                "NugetLocalPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Formatters",
                "Format and transform text",
                TextFormatIcon,
                "#AB47BC",
                "FormattersPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Clipboard Password",
                "Generate and copy passwords",
                LockIcon,
                "#EF5350",
                "ClipboardPasswordPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "EF Tools",
                "Entity Framework tools and utilities",
                DatabaseIcon,
                "#26A69A",
                "EFToolsPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Code Execute",
                "Run C# code snippets",
                TerminalIcon,
                "#42A5F5",
                "CodeExecutePage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Focus Timer",
                "Intelligent break scheduling",
                ClockIcon,
                "#FFA726",
                "FocusTimerSettingsPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "SnapIt",
                "Window snapping and management",
                GridLayoutIcon,
                "#78909C",
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
                IconPath = HomeIcon,
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