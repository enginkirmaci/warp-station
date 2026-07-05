namespace Tools.Library.Entities;

/// <summary>
/// Represents the cached repository data, persisted next to the executable so the
/// UI can render instantly on startup while a fresh scan runs in the background.
/// </summary>
public class RepoCache
{
    /// <summary>
    /// Gets or sets the list of cached repos (including their user-defined tags).
    /// </summary>
    public List<Repo> Repos { get; set; } = new();
}
