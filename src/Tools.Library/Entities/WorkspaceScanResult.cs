namespace Tools.Library.Entities;

/// <summary>
/// Holds the result of a workspace scan: discovered solutions (workspaces) and
/// platform folders, ready to be cached or displayed.
/// </summary>
public class WorkspaceScanResult
{
    /// <summary>
    /// Gets or sets the discovered solution (workspace) items.
    /// </summary>
    public List<WorkspaceItem> Workspaces { get; set; } = new();

    /// <summary>
    /// Gets or sets the discovered platform folder items.
    /// </summary>
    public List<WorkspaceItem> Platforms { get; set; } = new();
}
