using System.Windows.Input;

namespace Tools.Library.Models;

/// <summary>
/// Represents a navigation item in the application.
/// </summary>
public class NavigationItem
{
    /// <summary>
    /// Gets or sets the title of the navigation item.
    /// </summary>
    public required string Title { get; set; }

    /// <summary>
    /// Gets or sets the subtitle or description.
    /// </summary>
    public required string Subtitle { get; set; }

    /// <summary>
    /// Gets or sets the icon symbol (glyph).
    /// </summary>
    public required string Symbol { get; set; }

    /// <summary>
    /// Gets or sets the unique key identifying the target page.
    /// </summary>
    public required string PageKey { get; set; }

    /// <summary>
    /// Gets or sets the command to execute when the item is clicked.
    /// </summary>
    public required ICommand Command { get; set; }
}
