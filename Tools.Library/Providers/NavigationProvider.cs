using Microsoft.UI.Xaml.Controls;
using System.Windows.Input;
using Tools.Library.Models;

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
                "Workspaces",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Nuget Local Copy",
                "copy new nuget packages to destination",
                "\uE8DE", // BoxMultipleArrowRight20
                "NugetLocal",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Formatters",
                string.Empty,
                "\uE8D2", // TextNumberFormat24
                "Formatters",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Clipboard Password",
                "Generate and copy passwords",
                "\uE77F", // ClipboardPaste24
                "ClipboardPassword",
                cardClickCommand
            ),
            CreateNavigationItem(
                "EF Tools",
                "Entity Framework tools and utilities",
                "\uEE94", // Database24
                "EFTools",
                cardClickCommand
            ),
            CreateNavigationItem(
                "Code Execute",
                string.Empty,
                "\uE943", // Code24
                "CodeExecute",
                cardClickCommand
            )
        };
    }

    /// <summary>
    /// Gets navigation view items for the navigation menu.
    /// </summary>
    /// <returns>Collection of navigation view items.</returns>
    public static IReadOnlyCollection<NavigationViewItem> GetNavigationMenuItems()
    {
        var items = new List<NavigationViewItem>
        {
            new NavigationViewItem
            {
                Content = "Dashboard",
                Icon = new FontIcon { Glyph = "\uE80F" }, // Home24
                Tag = "Dashboard"
            }
        };

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
