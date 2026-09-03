namespace Tools.Library.Configuration;

/// <summary>
/// The sort orders offered on the Repos page list. Favorites always float to the top
/// regardless of the mode; the mode orders everything below them.
/// </summary>
public enum RepoSortMode
{
    /// <summary>Alphabetical by repo name (the historical default).</summary>
    Name = 0,

    /// <summary>Most recent commit first; repos without a known commit date last.</summary>
    LastActivity = 1,

    /// <summary>Most pending work first: working-tree changes, then to-push, then to-pull counts.</summary>
    Changes = 2,
}
