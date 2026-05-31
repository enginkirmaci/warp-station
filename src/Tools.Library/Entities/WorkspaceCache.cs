namespace Tools.Library.Entities;

/// <summary>
/// Represents the cached workspace data.
/// </summary>
public class WorkspaceCache
{
    /// <summary>
    /// Gets or sets the list of cached workspaces.
    /// </summary>
    public List<WorkspaceItem> Workspaces { get; set; } = new();

    /// <summary>
    /// Gets or sets the list of cached platforms.
    /// </summary>
    public List<WorkspaceItem> Platforms { get; set; } = new();
}
