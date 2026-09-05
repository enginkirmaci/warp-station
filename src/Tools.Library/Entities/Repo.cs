using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;

namespace Tools.Library.Entities;

/// <summary>
/// Represents a single repository (a discovered .git folder's parent) in the
/// application. The solution file, if present, becomes a property of the repo;
/// otherwise the Visual Studio action is disabled for it.
/// </summary>
public partial class Repo : ObservableObject
{
    /// <summary>
    /// Gets or sets the display name (the repo folder name).
    /// </summary>
    public string? Name { get; set; }

    /// <summary>
    /// Gets or sets the absolute path of the repo folder (the .git folder's parent).
    /// Used as the stable identity when merging user-defined tags across rescans.
    /// </summary>
    public string? FolderPath { get; set; }

    /// <summary>
    /// Gets or sets the absolute path of the solution file, if one was found.
    /// When <c>null</c> the repo has no Visual Studio action.
    /// </summary>
    public string? SolutionPath { get; set; }

    /// <summary>
    /// Gets the tags assigned to this repo. Each entry is a <see cref="RepoTag"/> that
    /// back-references this repo so chip commands can act on a single parameter.
    /// Auto-tags (e.g. <c>platform</c>) are recomputed on each scan; user tags are
    /// persisted in the cache and merged back after a rescan by matching
    /// <see cref="FolderPath"/>.
    /// </summary>
    public ObservableCollection<RepoTag> Tags { get; set; } = new();

    /// <summary>
    /// Convenience: adds a tag by name, ignoring duplicates (case-insensitive).
    /// </summary>
    public void AddTag(string name)
    {
        var trimmed = name?.Trim();
        if (string.IsNullOrEmpty(trimmed)) return;
        if (Tags.Any(t => string.Equals(t.Name, trimmed, StringComparison.OrdinalIgnoreCase))) return;
        Tags.Add(new RepoTag(this, trimmed));
    }

    /// <summary>
    /// Convenience: removes a tag by name (case-insensitive). Returns true if removed.
    /// </summary>
    public bool RemoveTag(string name)
    {
        var toRemove = Tags
            .Where(t => string.Equals(t.Name, name, StringComparison.OrdinalIgnoreCase))
            .ToList();
        foreach (var tag in toRemove)
            Tags.Remove(tag);
        return toRemove.Count > 0;
    }

    /// <summary>
    /// The reserved tag toggled by the star affordance. Defined here (rather than in
    /// <c>RepoService</c>) so <see cref="IsFavorite"/> can reference it without the
    /// entities layer depending on the services layer.
    /// </summary>
    public const string FavoritesTag = "favorites";

    /// <summary>
    /// The auto-tag applied by the scanner to repos whose folder path matches the
    /// configured platform folder name. Defined here so both the scanner (services
    /// layer) and UI consumers can reference it without coupling.
    /// </summary>
    public const string PlatformTag = "platform";

    /// <summary>
    /// True when the reserved <c>favorites</c> tag is present.
    /// </summary>
    public bool IsFavorite
        => Tags.Any(t => string.Equals(t.Name, FavoritesTag, StringComparison.OrdinalIgnoreCase));

    /// <summary>
    /// The current branch name reported by the local git status check, pushed by
    /// <c>IGitStatusService</c>. Shown next to the Git section header on the repo card.
    /// <c>null</c> until the first check completes (or when it fails). Runtime-only;
    /// not persisted.
    /// </summary>
    [ObservableProperty]
    private string? _gitBranchName;

    /// <summary>
    /// Whether this repo is the bottom bar's current selection — the table keeps its
    /// row highlighted while the bar acts on it. Pushed by the <c>BottomBarViewModel</c>
    /// on every selection change. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private bool _isBarSelected;

    /// <summary>
    /// Number of working-tree changes (modified, renamed, unmerged and untracked files)
    /// reported by <c>git status --porcelain=v2 --untracked-files=all</c>. Every
    /// untracked file counts individually (git's default directory collapsing is off).
    /// Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitModifiedCount;

    /// <summary>
    /// Number of local commits ahead of the upstream branch (to push). Zero when the
    /// branch has no upstream. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitToPushCount;

    /// <summary>
    /// Number of upstream commits the local branch is behind (to pull). Zero when the
    /// branch has no upstream. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitToPullCount;

    /// <summary>
    /// Whether the first git status check has completed for this repo. The card shows a
    /// "checking…" placeholder until this flips to <see langword="true"/>, so the UI
    /// renders instantly and the counts fill in as the background checks finish.
    /// Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private bool _gitStatusLoaded;

    /// <summary>
    /// Committer date of the most recent commit (<c>git log -1</c>), pushed by
    /// <c>IGitStatusService</c> alongside the status counts.
    /// <c>null</c> until the first check completes, or for repos with no commits (or when
    /// the check fails). Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private DateTimeOffset? _gitLastCommitAt;

    /// <summary>
    /// The relative-age label shown in the Last Activity column ("2h ago", "3w ago"),
    /// derived from <see cref="GitLastCommitAt"/>. <c>null</c> when no commit date is
    /// known. Refreshed only when a new value arrives, not on a timer — ages drift stale
    /// until the next refresh pass.
    /// </summary>
    public string? GitLastActivity => GitLastCommitAt is { } at ? FormatRelative(at) : null;

    partial void OnGitLastCommitAtChanged(DateTimeOffset? value) => OnPropertyChanged(nameof(GitLastActivity));

    /// <summary>
    /// When the repo was last fetched (<c>git fetch</c>), either run from the app (the
    /// bottom bar's Fetch button) or seeded from <c>.git/FETCH_HEAD</c>'s write time
    /// during a status pass. <c>null</c> when the repo has never been fetched.
    /// Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private DateTimeOffset? _gitLastFetchAt;

    /// <summary>
    /// The relative-age label shown beside the bottom bar's Fetch button ("2m ago"),
    /// derived from <see cref="GitLastFetchAt"/>. <c>null</c> when never fetched.
    /// </summary>
    public string? GitLastFetchLabel => GitLastFetchAt is { } at ? FormatRelative(at) : null;

    partial void OnGitLastFetchAtChanged(DateTimeOffset? value) => OnPropertyChanged(nameof(GitLastFetchLabel));

    // --- GitHub column (runtime-only, pushed by IGitHubService) ---

    /// <summary>
    /// The HTML URL of the repo on GitHub (e.g. <c>https://github.com/owner/repo</c>),
    /// pushed by <c>IGitHubService</c>. <c>null</c> when the repo has no GitHub remote
    /// or the probe failed — the GitHub column then shows nothing for this repo.
    /// Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private string? _gitHubRepoUrl;

    /// <summary>
    /// Number of open pull requests reported by <c>gh pr list</c> (capped at the fetch
    /// limit). Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitHubPrCount;

    /// <summary>
    /// Number of open issues reported by <c>gh issue list</c> (capped at the fetch
    /// limit). Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitHubIssueCount;

    /// <summary>
    /// Whether the GitHub probe has completed for this repo — gates the column's
    /// placeholder-free empty state the same way <see cref="GitStatusLoaded"/> does for
    /// the git cells. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private bool _gitHubLoaded;

    /// <summary>
    /// Whether the repo resolved to a GitHub repository at all (a successful
    /// <c>gh repo view</c>). Non-GitHub repos keep the cell empty instead of showing a
    /// misleading "OK". Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private bool _gitHubAvailable;

    /// <summary>Whether the open pull-requests chip shows in the GitHub column.</summary>
    public bool HasGitHubPullRequests => GitHubPrCount > 0;

    /// <summary>Whether the open issues chip shows in the GitHub column.</summary>
    public bool HasGitHubIssues => GitHubIssueCount > 0;

    /// <summary>
    /// Whether the GitHub column shows the all-clear "OK" state for this repo: probed,
    /// a GitHub repo, and nothing open at all.
    /// </summary>
    public bool IsGitHubAllClear
        => GitHubLoaded && GitHubAvailable && GitHubPrCount == 0 && GitHubIssueCount == 0;

    partial void OnGitHubPrCountChanged(int value)
    {
        OnPropertyChanged(nameof(HasGitHubPullRequests));
        OnPropertyChanged(nameof(IsGitHubAllClear));
    }

    partial void OnGitHubIssueCountChanged(int value)
    {
        OnPropertyChanged(nameof(HasGitHubIssues));
        OnPropertyChanged(nameof(IsGitHubAllClear));
    }

    partial void OnGitHubLoadedChanged(bool value) => OnPropertyChanged(nameof(IsGitHubAllClear));

    partial void OnGitHubAvailableChanged(bool value) => OnPropertyChanged(nameof(IsGitHubAllClear));

    // --- Azure DevOps column (runtime-only, pushed by IAzureDevOpsService) ---

    /// <summary>
    /// The HTML URL of the repo on Azure DevOps, pushed by <c>IAzureDevOpsService</c>.
    /// <c>null</c> when the repo has no Azure DevOps remote or the probe failed — the
    /// column then shows nothing for this repo. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private string? _azureDevOpsRepoUrl;

    /// <summary>
    /// Number of active pull requests reported by the Azure DevOps REST API (capped at
    /// the fetch limit). Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _azureDevOpsPrCount;

    /// <summary>
    /// Number of open work items in the repo's hosting Azure DevOps project (capped at
    /// the fetch limit). Work items are not repo-scoped in Azure DevOps, so the count is
    /// project-wide. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _azureDevOpsWorkItemCount;

    /// <summary>
    /// The latest pipeline run's state for this repo: its <c>result</c> when completed
    /// (<c>succeeded</c>, <c>failed</c>, …) or its <c>status</c> while still running
    /// (<c>inProgress</c>, …). <c>null</c> when the repo has no pipeline runs (or is not
    /// on Azure DevOps). Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private string? _azureDevOpsPipelineState;

    /// <summary>
    /// One-line summary of the latest pipeline run for the column chip's tooltip
    /// ("Build #472 'CI' — succeeded, 12m ago"). <c>null</c> with
    /// <see cref="AzureDevOpsPipelineState"/>. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private string? _azureDevOpsPipelineInfo;

    /// <summary>
    /// Whether the Azure DevOps probe has completed for this repo — gates the column's
    /// placeholder-free empty state the same way <see cref="GitStatusLoaded"/> does for
    /// the git cells. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private bool _azureDevOpsLoaded;

    /// <summary>
    /// Whether the repo resolved to an Azure DevOps repository at all (remote parsed and
    /// the API recognized it). Other hosts keep the cell empty instead of showing a
    /// misleading all-clear. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private bool _azureDevOpsAvailable;

    /// <summary>Whether the active pull-requests chip shows in the Azure DevOps column.</summary>
    public bool HasAzureDevOpsPullRequests => AzureDevOpsPrCount > 0;

    /// <summary>Whether the open work-items chip shows in the Azure DevOps column.</summary>
    public bool HasAzureDevOpsWorkItems => AzureDevOpsWorkItemCount > 0;

    /// <summary>Whether the latest-pipeline-run chip shows in the Azure DevOps column.</summary>
    public bool HasAzureDevOpsPipeline => !string.IsNullOrWhiteSpace(AzureDevOpsPipelineState);

    /// <summary>Whether the latest pipeline run finished unsuccessfully (drives the chip's failure color).</summary>
    public bool IsAzureDevOpsPipelineFailed
        => string.Equals(AzureDevOpsPipelineState, "failed", StringComparison.OrdinalIgnoreCase)
        || string.Equals(AzureDevOpsPipelineState, "canceled", StringComparison.OrdinalIgnoreCase);

    /// <summary>Whether the latest pipeline run finished successfully (drives the chip's success color).</summary>
    public bool IsAzureDevOpsPipelineOk
        => HasAzureDevOpsPipeline && !IsAzureDevOpsPipelineFailed && !IsAzureDevOpsPipelineRunning;

    /// <summary>Whether the latest pipeline run is still in flight (drives the chip's running color).
    /// False when there is no pipeline data at all — "unknown" must not read as "running".</summary>
    public bool IsAzureDevOpsPipelineRunning
        => HasAzureDevOpsPipeline && !IsAzureDevOpsPipelineFailed
        && !string.Equals(AzureDevOpsPipelineState, "succeeded", StringComparison.OrdinalIgnoreCase)
        && !string.Equals(AzureDevOpsPipelineState, "partiallySucceeded", StringComparison.OrdinalIgnoreCase);

    /// <summary>
    /// Whether the Azure DevOps column shows the all-clear "OK" state for this repo:
    /// probed, hosted on Azure DevOps, nothing open and no failing pipeline. A repo
    /// without any pipeline runs still qualifies (no CI is not a problem).
    /// </summary>
    public bool IsAzureDevOpsAllClear
        => AzureDevOpsLoaded && AzureDevOpsAvailable && AzureDevOpsPrCount == 0 && AzureDevOpsWorkItemCount == 0
        && (!HasAzureDevOpsPipeline || !IsAzureDevOpsPipelineFailed);

    partial void OnAzureDevOpsPrCountChanged(int value)
    {
        OnPropertyChanged(nameof(HasAzureDevOpsPullRequests));
        OnPropertyChanged(nameof(IsAzureDevOpsAllClear));
    }

    partial void OnAzureDevOpsWorkItemCountChanged(int value)
    {
        OnPropertyChanged(nameof(HasAzureDevOpsWorkItems));
        OnPropertyChanged(nameof(IsAzureDevOpsAllClear));
    }

    partial void OnAzureDevOpsPipelineStateChanged(string? value)
    {
        OnPropertyChanged(nameof(HasAzureDevOpsPipeline));
        OnPropertyChanged(nameof(IsAzureDevOpsPipelineFailed));
        OnPropertyChanged(nameof(IsAzureDevOpsPipelineRunning));
        OnPropertyChanged(nameof(IsAzureDevOpsPipelineOk));
        OnPropertyChanged(nameof(IsAzureDevOpsAllClear));
    }

    partial void OnAzureDevOpsLoadedChanged(bool value) => OnPropertyChanged(nameof(IsAzureDevOpsAllClear));

    partial void OnAzureDevOpsAvailableChanged(bool value) => OnPropertyChanged(nameof(IsAzureDevOpsAllClear));

    /// <summary>
    /// Formats an age as a compact relative label matching the Repos table's style:
    /// the most significant unit only, no rounding up across unit boundaries
    /// (<c>just now</c>, <c>5m ago</c>, <c>2h ago</c>, <c>1d ago</c>, <c>3w ago</c>,
    /// <c>1mo ago</c>, <c>1y ago</c>). Uses wall-clock difference, so the label is
    /// unaffected by the commit's UTC offset.
    /// </summary>
    private static string FormatRelative(DateTimeOffset at)
    {
        var span = DateTimeOffset.Now - at;
        var minutes = (int)(span.Ticks < 0 ? 0 : span.TotalMinutes);
        return minutes switch
        {
            < 1 => "just now",
            < 60 => $"{minutes}m ago",
            _ when minutes < 60 * 24 => $"{minutes / 60}h ago",
            _ when minutes < 60 * 24 * 7 => $"{minutes / (60 * 24)}d ago",
            _ when minutes < 60 * 24 * 30 => $"{minutes / (60 * 24 * 7)}w ago",
            _ when minutes < 60 * 24 * 365 => $"{minutes / (60 * 24 * 30)}mo ago",
            _ => $"{minutes / (60 * 24 * 365)}y ago",
        };
    }
}
