namespace Tools.Library.Entities;

/// <summary>
/// One open pull request or issue as reported by the <c>gh</c> CLI, carried into the
/// GitHub details dialog. <see cref="IsDraft"/> is only meaningful for pull requests.
/// </summary>
public sealed record GitHubItem(
    int Number,
    string Title,
    string Url,
    string? Author,
    IReadOnlyList<string> Labels,
    bool IsDraft)
{
    /// <summary>Whether any label chips show under the item's title.</summary>
    public bool HasLabels => Labels.Count > 0;
}
