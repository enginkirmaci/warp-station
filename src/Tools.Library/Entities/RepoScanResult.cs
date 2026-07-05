namespace Tools.Library.Entities;

/// <summary>
/// The in-memory result of a filesystem scan, before it is merged with cached
/// user-defined tags and persisted.
/// </summary>
public class RepoScanResult
{
    /// <summary>
    /// Gets or sets the list of discovered repos.
    /// </summary>
    public List<Repo> Repos { get; set; } = new();
}
