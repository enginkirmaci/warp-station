using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the <see cref="Views.Windows.AzureDevOpsDetailsDialog"/>. Shows one
/// repo's active pull requests, open work items and recent pipeline runs in a single
/// scrollable list — no tabs. Data comes from <see cref="IAzureDevOpsService"/>: the
/// cached activity renders the dialog instantly, then a fresh REST fetch runs (and can
/// be re-run via the dialog's Refresh button). Every row and the header's repo link
/// open through <see cref="IProcessLauncher"/>.
/// </summary>
public partial class AzureDevOpsDetailsViewModel : ObservableObject
{
    private readonly Repo _repo;
    private readonly IAzureDevOpsService _azureDevOpsService;
    private readonly IProcessLauncher _processLauncher;

    [ObservableProperty]
    private ObservableCollection<AzureDevOpsItem> _pullRequests = new();

    [ObservableProperty]
    private ObservableCollection<AzureDevOpsItem> _workItems = new();

    [ObservableProperty]
    private ObservableCollection<AzureDevOpsPipelineRun> _pipelineRuns = new();

    /// <summary>
    /// True while the REST fetch for this repo is in flight; gates the Refresh button
    /// (clicks coalesce) and the first-load hint.
    /// </summary>
    [ObservableProperty]
    private bool _isRefreshing;

    /// <summary>Whether at least one fetch has completed; gates the empty state so it
    /// does not flash "OK" before the first fetch returns.</summary>
    [ObservableProperty]
    private bool _hasLoaded;

    /// <summary>
    /// Whether the last fetch could not resolve the repo on Azure DevOps (no Azure
    /// DevOps remote, token missing/rejected, or a failed fetch). Shows an honest "no
    /// data" note instead of the all-clear state, and keeps any previously loaded
    /// lists on screen.
    /// </summary>
    [ObservableProperty]
    private bool _isUnavailable;

    public AzureDevOpsDetailsViewModel(
        Repo repo,
        IAzureDevOpsService azureDevOpsService,
        IProcessLauncher processLauncher)
    {
        _repo = repo;
        _azureDevOpsService = azureDevOpsService;
        _processLauncher = processLauncher;

        // Render from the column's last fetch when available, so opening the dialog is
        // instant; the fresh fetch below replaces the lists when it returns.
        var cached = azureDevOpsService.GetCachedActivity(repo);
        if (cached is not null)
        {
            ApplyActivity(cached);
        }

        _ = RefreshAsync();
    }

    /// <summary>The repo name shown in the dialog title.</summary>
    public string RepoName => _repo.Name ?? string.Empty;

    /// <summary>The repo's Azure DevOps web URL, or null for non-Azure-DevOps repos (hides the link).</summary>
    public string? RepoUrl => _repo.AzureDevOpsRepoUrl;

    /// <summary>Whether the pull-requests section shows (at least one active PR).</summary>
    public bool HasPullRequests => PullRequests.Count > 0;

    /// <summary>Whether the work-items section shows (at least one open item).</summary>
    public bool HasWorkItems => WorkItems.Count > 0;

    /// <summary>Whether the pipeline-runs section shows (at least one recent run).</summary>
    public bool HasPipelineRuns => PipelineRuns.Count > 0;

    /// <summary>Whether the all-clear empty state shows: loaded, an Azure DevOps repo, and
    /// nothing open at all.</summary>
    public bool HasNoItems
        => HasLoaded && !IsUnavailable && PullRequests.Count == 0 && WorkItems.Count == 0 && PipelineRuns.Count == 0;

    /// <summary>Whether the "no Azure DevOps data" note shows: loaded, unavailable, and no
    /// previously fetched lists to fall back on.</summary>
    public bool ShowUnavailableNote
        => HasLoaded && IsUnavailable && !HasPullRequests && !HasWorkItems && !HasPipelineRuns;

    partial void OnPullRequestsChanged(ObservableCollection<AzureDevOpsItem> value)
    {
        OnPropertyChanged(nameof(HasPullRequests));
        OnPropertyChanged(nameof(HasNoItems));
        OnPropertyChanged(nameof(ShowUnavailableNote));
    }

    partial void OnWorkItemsChanged(ObservableCollection<AzureDevOpsItem> value)
    {
        OnPropertyChanged(nameof(HasWorkItems));
        OnPropertyChanged(nameof(HasNoItems));
        OnPropertyChanged(nameof(ShowUnavailableNote));
    }

    partial void OnPipelineRunsChanged(ObservableCollection<AzureDevOpsPipelineRun> value)
    {
        OnPropertyChanged(nameof(HasPipelineRuns));
        OnPropertyChanged(nameof(HasNoItems));
        OnPropertyChanged(nameof(ShowUnavailableNote));
    }

    partial void OnHasLoadedChanged(bool value)
    {
        OnPropertyChanged(nameof(HasNoItems));
        OnPropertyChanged(nameof(ShowUnavailableNote));
    }

    partial void OnIsUnavailableChanged(bool value)
    {
        OnPropertyChanged(nameof(HasNoItems));
        OnPropertyChanged(nameof(ShowUnavailableNote));
    }

    /// <summary>
    /// Fetches the repo's activity fresh from the Azure DevOps REST API and replaces
    /// all three lists. Re-entrant clicks are coalesced via <see cref="CanRefresh"/>.
    /// Failures leave the current lists untouched (the service logs) — the dialog just
    /// stops spinning.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanRefresh))]
    private async Task RefreshAsync()
    {
        IsRefreshing = true;
        try
        {
            var activity = await _azureDevOpsService.RefreshRepoAsync(_repo);
            // A failed fetch (no Azure DevOps remote, token missing/rejected, network
            // down) returns an empty activity — applying it would blank the lists and
            // flash a misleading all-clear. Keep the previous lists and surface the
            // unavailable note.
            IsUnavailable = !_repo.AzureDevOpsAvailable;
            ApplyActivity(activity);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Azure DevOps details refresh failed for {FolderPath}", _repo.FolderPath);
        }
        finally
        {
            IsRefreshing = false;
        }
    }

    /// <summary>A refresh can start only when one isn't already running.</summary>
    private bool CanRefresh() => !IsRefreshing;

    /// <summary>Opens the clicked pull request / work item in the browser.</summary>
    [RelayCommand]
    private void OpenItem(AzureDevOpsItem? item)
    {
        if (!string.IsNullOrWhiteSpace(item?.Url))
        {
            _processLauncher.StartProcess(item.Url);
        }
    }

    /// <summary>Opens the clicked pipeline run in the browser.</summary>
    [RelayCommand]
    private void OpenPipeline(AzureDevOpsPipelineRun? run)
    {
        if (!string.IsNullOrWhiteSpace(run?.Url))
        {
            _processLauncher.StartProcess(run.Url);
        }
    }

    /// <summary>Opens the repo's Azure DevOps page in the browser.</summary>
    [RelayCommand]
    private void OpenRepo()
    {
        if (!string.IsNullOrWhiteSpace(RepoUrl))
        {
            _processLauncher.StartProcess(RepoUrl);
        }
    }

    private void ApplyActivity(AzureDevOpsActivity activity)
    {
        PullRequests = new ObservableCollection<AzureDevOpsItem>(activity.PullRequests);
        WorkItems = new ObservableCollection<AzureDevOpsItem>(activity.WorkItems);
        PipelineRuns = new ObservableCollection<AzureDevOpsPipelineRun>(activity.PipelineRuns);
        OnPropertyChanged(nameof(RepoUrl));
        HasLoaded = true;
    }
}
