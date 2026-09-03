namespace Tools.Library.Entities;

/// <summary>
/// One pipeline (build) run as reported by the Azure DevOps REST API, carried into the
/// Azure DevOps details dialog. <see cref="Result"/> is only set once the run completed
/// (<c>succeeded</c>, <c>failed</c>, <c>canceled</c>, …); a running run has
/// <see cref="IsRunning"/> instead.
/// </summary>
public sealed record AzureDevOpsPipelineRun(
    int Id,
    string BuildNumber,
    string? DefinitionName,
    string? Branch,
    string? Result,
    string? Status,
    string? RequestedBy,
    string? Url,
    DateTimeOffset? FinishTime)
{
    /// <summary>Azure DevOps build status values meaning the run has not finished yet.</summary>
    private static readonly string[] RunningStatuses = ["notStarted", "inProgress", "postponed"];

    /// <summary>Whether the run has not finished yet (drives the "running" visuals).</summary>
    public bool IsRunning => !IsFailed && !IsSucceeded && RunningStatuses.Contains(Status, StringComparer.OrdinalIgnoreCase);

    /// <summary>Whether the run completed with a failure or cancellation.</summary>
    public bool IsFailed => string.Equals(Result, "failed", StringComparison.OrdinalIgnoreCase)
        || string.Equals(Result, "canceled", StringComparison.OrdinalIgnoreCase);

    /// <summary>Whether the run completed successfully.</summary>
    public bool IsSucceeded => string.Equals(Result, "succeeded", StringComparison.OrdinalIgnoreCase)
        || string.Equals(Result, "partiallySucceeded", StringComparison.OrdinalIgnoreCase);

    /// <summary>The branch name for display: the <c>refs/heads/</c> prefix trimmed off.</summary>
    public string? DisplayBranch => Branch?.StartsWith("refs/heads/", StringComparison.Ordinal) == true
        ? Branch["refs/heads/".Length..]
        : Branch;
}
