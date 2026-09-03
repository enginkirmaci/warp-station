using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// The Azure DevOps activity fetched for one repo, in dialog display order:
/// pull requests, work items (the hosting project's open items), then the repo's most
/// recent pipeline runs (newest first).
/// </summary>
public sealed record AzureDevOpsActivity(
    IReadOnlyList<AzureDevOpsItem> PullRequests,
    IReadOnlyList<AzureDevOpsItem> WorkItems,
    IReadOnlyList<AzureDevOpsPipelineRun> PipelineRuns)
{
    public static readonly AzureDevOpsActivity Empty = new([], [], []);
}
