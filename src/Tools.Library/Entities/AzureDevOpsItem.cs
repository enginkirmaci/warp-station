namespace Tools.Library.Entities;

/// <summary>
/// One open pull request or work item as reported by the Azure DevOps REST API, carried
/// into the Azure DevOps details dialog. <see cref="IsDraft"/> is only meaningful for
/// pull requests; <see cref="State"/> is only meaningful for work items.
/// </summary>
public sealed record AzureDevOpsItem(
    int Number,
    string Title,
    string Url,
    string? Author,
    IReadOnlyList<string> Labels,
    bool IsDraft,
    string? State)
{
    /// <summary>Whether any label chips show under the item's title.</summary>
    public bool HasLabels => Labels.Count > 0;
}
