using System.Windows.Input;

namespace Tools.Library.Entities;

/// <summary>
/// Represents a navigation item in the application.
/// </summary>
public class NavigationItem
{
    /// <summary>
    /// Gets or sets the title of the navigation item.
    /// </summary>
    public string Title { get; set; }

    /// <summary>
    /// Gets or sets the subtitle or description.
    /// </summary>
    public string Subtitle { get; set; }

    /// <summary>
    /// Gets or sets the icon symbol (glyph).
    /// </summary>
    public string Symbol { get; set; }

    /// <summary>
    /// Gets or sets the SVG path data for the icon (e.g. "M12,3 L2,12...").
    /// </summary>
    public string IconPath { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the accent color hex for the icon background (e.g. "#5B8DEF").
    /// </summary>
    public string AccentColor { get; set; } = "#5B8DEF";

    /// <summary>
    /// Gets or sets the unique key identifying the target page.
    /// </summary>
    public string PageKey { get; set; }

    /// <summary>
    /// Gets or sets the command to execute when the item is clicked.
    /// </summary>
    public ICommand Command { get; set; }
}