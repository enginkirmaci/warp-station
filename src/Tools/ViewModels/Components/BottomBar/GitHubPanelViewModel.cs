using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// The GitHub panel's data: the selected repo's open pull requests and issues (the
/// Pull Requests and Issues tabs list them; the Overview cards show cached five-item
/// slices), the load/refresh flags and the github.com jump commands. The tab header
/// totals and the shared-tabs' empty-state notes read this panel next to the Azure
/// one — the shell re-raises those aggregates on <see cref="StateChanged"/>.
/// </summary>
public sealed partial class GitHubPanelViewModel : BottomBarPanelViewModel
{
    public GitHubPanelViewModel(BottomBarViewModel shell) : base(shell)
    {
    }

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _pullRequests = new();

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _issues = new();

    /// <summary>First five pull requests / issues for the Overview cards (tabs list all).
    /// Cached slices — recomputed only when the lists (re)fill, instead of re-enumerating
    /// the collections on every binding evaluation.</summary>
    public IReadOnlyList<GitHubItem> PullRequestsPreview => _pullRequestsPreview;

    public IReadOnlyList<GitHubItem> IssuesPreview => _issuesPreview;

    private IReadOnlyList<GitHubItem> _pullRequestsPreview = Array.Empty<GitHubItem>();

    private IReadOnlyList<GitHubItem> _issuesPreview = Array.Empty<GitHubItem>();

    /// <summary>Recomputes the Overview cards' GitHub preview slices and raises their
    /// bindings — called wherever the lists are (re)filled or cleared.</summary>
    private void RefreshPreviews()
    {
        _pullRequestsPreview = PullRequests.Take(5).ToList();
        _issuesPreview = Issues.Take(5).ToList();
        OnPropertyChanged(nameof(PullRequestsPreview));
        OnPropertyChanged(nameof(IssuesPreview));
    }

    [ObservableProperty]
    private bool _isRefreshing;

    [ObservableProperty]
    private bool _hasLoaded;

    [ObservableProperty]
    private bool _isUnavailable;

    /// <summary>The provider's unavailable note: only once settled (loaded) — otherwise
    /// it would sit above rows still loading.</summary>
    public bool ShowUnavailable => HasLoaded && IsUnavailable;

    /// <summary>Loads the GitHub panel: the cached lists first (instant tab), then the
    /// refresh. A missing repo clears — the bar shows nothing without a selection.</summary>
    public Task LoadAsync()
    {
        var repo = Repo;
        if (repo is null)
        {
            PullRequests.Clear();
            Issues.Clear();
            RefreshPreviews();
            RaiseStateChanged();
            return Task.CompletedTask;
        }

        return LoadProviderAsync(
            () => Shell.GitHubService.GetCachedActivity(repo),
            ApplyActivity,
            RefreshAsync);
    }

    /// <summary>Refreshes the GitHub pull requests and issues from github.com.</summary>
    [RelayCommand(CanExecute = nameof(CanRefresh))]
    private Task RefreshAsync() => RefreshProviderAsync(
        "GitHub",
        () => true,
        value => IsRefreshing = value,
        repo => Shell.GitHubService.RefreshRepoAsync(repo),
        repo => IsUnavailable = !repo.GitHubAvailable,
        ApplyActivity);

    private bool CanRefresh() => !IsRefreshing;

    private void ApplyActivity(GitHubActivity activity)
    {
        // In-place sync: the same collection instances keep serving the ItemsControls,
        // so a refresh recycles rows instead of regenerating every container.
        ReplaceItems(PullRequests, activity.PullRequests);
        ReplaceItems(Issues, activity.Issues);
        HasLoaded = true;
        RefreshPreviews();
        RaiseStateChanged();
    }

    /// <summary>Opens the clicked pull request / issue on github.com.</summary>
    [RelayCommand]
    private void OpenItem(GitHubItem? item)
    {
        if (!string.IsNullOrWhiteSpace(item?.Url))
        {
            Shell.ProcessLauncher.StartProcess(item.Url);
        }
    }
}
