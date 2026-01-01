namespace Tools.Library.Entities;

/// <summary>
/// Represents a workspace item in the application.
/// </summary>
public class WorkspaceItem
{
    /// <summary>
    /// Gets or sets the solution name.
    /// </summary>
    public string? SolutionName { get; set; }

    /// <summary>
    /// Gets or sets the platform name.
    /// </summary>
    public string? PlatformName { get; set; }

    /// <summary>
    /// Gets or sets the folder path.
    /// </summary>
    public string? FolderPath { get; set; }

    /// <summary>
    /// Gets or sets the solution file path.
    /// </summary>
    public string? SolutionPath { get; set; }
}
