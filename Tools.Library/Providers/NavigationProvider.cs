using Microsoft.UI.Xaml.Controls;
using System.Windows.Input;
using Tools.Library.Entities;

namespace Tools.Library.Providers;

/// <summary>
/// Provides navigation item collections for the application.
/// </summary>
public static class NavigationProvider
{
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
                "\uE8B7", // AppFolder24
                "WorkspacesPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Nuget Package Manager",
                "copy new nuget packages to destination",
                "\uE8DE", // BoxMultipleArrowRight20
                "NugetLocalPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Formatters",
                string.Empty,
                "\uE8D2", // TextNumberFormat24
                "FormattersPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Clipboard Password",
                "Generate and copy passwords",
                "\uE77F", // ClipboardPaste24
                "ClipboardPasswordPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "EF Tools",
                "Entity Framework tools and utilities",
                "\uEE94", // Database24
                "EFToolsPage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Code Execute",
                string.Empty,
                "\uE943", // Code24
                "CodeExecutePage",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Focus Timer",
                "Intelligent break scheduling",
                "\uE916", // Timer24
                "FocusTimerSettingsPage",
                cardClickCommand
            )
        };
    }

    /// <summary>
    /// Gets navigation view items for the navigation menu.
    /// </summary>
    /// <returns>Collection of navigation view items.</returns>
    public static IReadOnlyCollection<NavigationViewItemBase> GetNavigationMenuItems()
    {
        var items = new List<NavigationViewItemBase>
        {
            new NavigationViewItem
            {
                Content = "Dashboard",
                Icon = new FontIcon { Glyph = "\uE80F" }, // Home24
                Tag = "DashboardPage"
            }
        };

        //if (item.PageKey == "WorkspacesPage")
        {
            /*		<NavigationViewItemSeparator />
			<NavigationViewItemHeader Content="Tools" />*/
            items.Add(new NavigationViewItemSeparator());
            items.Add(new NavigationViewItemHeader
            {
                Content = "Tools"
            });
        }

        foreach (var item in GetDashboardItems(null))
        {
            items.Add(new NavigationViewItem
            {
                Content = item.Title,
                Icon = new FontIcon { Glyph = item.Symbol },
                Tag = item.PageKey
            });
        }

        return items;
    }

    private static NavigationItem CreateNavigationItem(
        string title,
        string subtitle,
        string symbol,
        string pageKey,
        ICommand? command)
    {
        return new NavigationItem
        {
            Title = title,
            Subtitle = subtitle,
            Symbol = symbol,
            PageKey = pageKey,
            Command = command!
        };
    }
}