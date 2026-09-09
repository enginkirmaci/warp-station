using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// The Azure DevOps panel's data: the selected repo's pull requests, work items and
/// pipeline runs (the Azure tab lists all three; the GitHub tabs and the Overview
/// cards show the first two), the load/refresh flags and the azure.com jump
/// commands. The shell re-raises the tab header totals and the Pipelines card's
/// health line when <see cref="StateChanged"/> fires.
/// </summary>
public sealed partial class AzurePanelViewModel : BottomBarPanelViewModel
{
    public AzurePanelViewModel(BottomBarViewModel shell) : base(shell)
    {
    }

    [ObservableProperty]
    private ObservableCollection<AzureDevOpsItem> _azurePullRequests = new();

    [ObservableProperty]
    private ObservableCollection<AzureDevOpsItem> _azureWorkItems = new();

    [ObservableProperty]
    private ObservableCollection<AzureDevOpsPipelineRun> _azurePipelineRuns = new();

    [ObservableProperty]
    private bool _isAzureRefreshing;

    [ObservableProperty]
    private bool _azureHasLoaded;

    [ObservableProperty]
    private bool _azureIsUnavailable;

    public bool HasAzurePullRequests => AzurePullRequests.Count > 0;
    public bool HasAzureWorkItems => AzureWorkItems.Count > 0;
    public bool HasAzurePipelineRuns => AzurePipelineRuns.Count > 0;

    /// <summary>First three Azure items for the Overview cards' Azure sections. Cached
    /// slices — recomputed only when the lists (re)fill, instead of re-enumerating
    /// the collections on every binding evaluation.</summary>
    public IReadOnlyList<AzureDevOpsItem> AzurePullRequestsPreview => _azurePullRequestsPreview;

    public IReadOnlyList<AzureDevOpsItem> AzureWorkItemsPreview => _azureWorkItemsPreview;

    private IReadOnlyList<AzureDevOpsItem> _azurePullRequestsPreview = Array.Empty<AzureDevOpsItem>();

    private IReadOnlyList<AzureDevOpsItem> _azureWorkItemsPreview = Array.Empty<AzureDevOpsItem>();

    /// <summary>Recomputes the Overview cards' Azure preview slices and raises their
    /// bindings — called wherever the lists are (re)filled or cleared.</summary>
    private void RefreshAzurePreviews()
    {
        _azurePullRequestsPreview = AzurePullRequests.Take(3).ToList();
        _azureWorkItemsPreview = AzureWorkItems.Take(3).ToList();
        OnPropertyChanged(nameof(AzurePullRequestsPreview));
        OnPropertyChanged(nameof(AzureWorkItemsPreview));
    }

    public bool ShowAzureEmpty => AzureHasLoaded && !AzureIsUnavailable
        && AzurePullRequests.Count == 0 && AzureWorkItems.Count == 0 && AzurePipelineRuns.Count == 0;
    public bool ShowAzureUnavailable => AzureHasLoaded && AzureIsUnavailable
        && !HasAzurePullRequests && !HasAzureWorkItems && !HasAzurePipelineRuns;

    /// <summary>Loads the Azure panel: the cached lists first, then the refresh. A
    /// missing repo clears — the bar shows nothing without a selection.</summary>
    public Task LoadAzureAsync()
    {
        var repo = Repo;
        if (repo is null)
        {
            AzurePullRequests.Clear();
            AzureWorkItems.Clear();
            AzurePipelineRuns.Clear();
            RaiseHasFlagsChanged();
            RefreshAzurePreviews();
            RaiseStateChanged();
            return Task.CompletedTask;
        }

        return LoadProviderAsync(
            () => Shell.AzureDevOpsService.GetCachedActivity(repo),
            ApplyAzureActivity,
            RefreshAzureAsync);
    }

    /// <summary>Refreshes the Azure pull requests, work items and pipeline runs.</summary>
    [RelayCommand(CanExecute = nameof(CanRefreshAzure))]
    private Task RefreshAzureAsync() => RefreshProviderAsync(
        "Azure",
        () => !IsAzureRefreshing,
        value => IsAzureRefreshing = value,
        repo => Shell.AzureDevOpsService.RefreshRepoAsync(repo),
        repo => AzureIsUnavailable = !repo.AzureDevOpsAvailable,
        ApplyAzureActivity);

    private bool CanRefreshAzure() => !IsAzureRefreshing;

    private void ApplyAzureActivity(AzureDevOpsActivity activity)
    {
        // In-place sync: the same collection instances keep serving the ItemsControls,
        // so a refresh recycles rows instead of regenerating every container.
        ReplaceItems(AzurePullRequests, activity.PullRequests);
        ReplaceItems(AzureWorkItems, activity.WorkItems);
        ReplaceItems(AzurePipelineRuns, activity.PipelineRuns);
        AzureHasLoaded = true;
        RefreshAzurePreviews();
        RaiseHasFlagsChanged();
        RaiseStateChanged();
    }

    /// <summary>The count-derived section gates — the collections mutate in place, so
    /// these need an explicit push after every apply/clear.</summary>
    private void RaiseHasFlagsChanged()
    {
        OnPropertyChanged(nameof(HasAzurePullRequests));
        OnPropertyChanged(nameof(HasAzureWorkItems));
        OnPropertyChanged(nameof(HasAzurePipelineRuns));
        OnPropertyChanged(nameof(ShowAzureEmpty));
        OnPropertyChanged(nameof(ShowAzureUnavailable));
    }

    /// <summary>Opens the clicked pull request / work item in the browser.</summary>
    [RelayCommand]
    private void OpenAzureItem(AzureDevOpsItem? item)
    {
        if (!string.IsNullOrWhiteSpace(item?.Url))
        {
            Shell.ProcessLauncher.StartProcess(item.Url);
        }
    }

    /// <summary>Opens the clicked pipeline run in the browser.</summary>
    [RelayCommand]
    private void OpenAzurePipeline(AzureDevOpsPipelineRun? run)
    {
        if (!string.IsNullOrWhiteSpace(run?.Url))
        {
            Shell.ProcessLauncher.StartProcess(run.Url);
        }
    }

    /// <summary>Opens the selected repo's Azure DevOps page in the browser.</summary>
    [RelayCommand]
    private void OpenAzureRepo()
    {
        if (!string.IsNullOrWhiteSpace(Shell.SelectedRepo?.AzureDevOpsRepoUrl))
        {
            Shell.ProcessLauncher.StartProcess(Shell.SelectedRepo.AzureDevOpsRepoUrl);
        }
    }
}
