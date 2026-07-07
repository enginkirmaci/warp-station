using System.Text.Json.Serialization;

namespace Tools.Library.Entities;

/// <summary>
/// A tag attached to a <see cref="Repo"/>. Carries a back-reference to its owner so a
/// tag chip's remove button can pass a single <see cref="RepoTag"/> as its
/// CommandParameter and the command knows both which tag and which repo.
/// </summary>
public class RepoTag
{
    /// <summary>
    /// Gets the repo this tag belongs to. Excluded from JSON serialization because it
    /// forms a reference cycle with <see cref="Repo.Tags"/> (which would otherwise throw
    /// <c>JsonException</c> on save); it is re-parented in memory when the cache loads
    /// (see <c>RepoService.EnsureLoadedAsync</c>).
    /// </summary>
    [JsonIgnore]
    public Repo Repo { get; }

    /// <summary>Gets the tag name.</summary>
    public string Name { get; }

    public RepoTag(Repo repo, string name)
    {
        Repo = repo;
        Name = name;
    }
}
