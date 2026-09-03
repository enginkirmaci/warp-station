using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the <see cref="Views.Windows.GitHubDetailsDialog"/>. Shows one repo's
/// open pull requests and issues (pull requests first) in a single scrollable list —
/// no tabs. Data comes from <see cref="IGitHubService"/>: the cached activity renders
/// the dialog instantly, then a fresh <c>gh</c> fetch runs (and can be re-run via the
/// dialog's Refresh button). Every row and the header's GitHub link open through
/// <see cref="IProcessLauncher"/>.
/// </summary>
public partial class GitHubDetailsViewModel : ObservableObject
{
    private readonly Repo _repo;
    private readonly IGitHubService _gitHubService;
    private readonly IProcessLauncher _processLauncher;

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _pullRequests = new();

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _issues = new();

    /// <summary>
    /// True while a <c>gh</c> fetch for this repo is in flight; gates the Refresh
    /// button (clicks coalesce) and the first-load hint.
    /// </summary>
    [ObservableProperty]
    private bool _isRefreshing;

    /// <summary>Whether at least one fetch has completed; gates the empty state so it
    /// does not flash "OK" before the first fetch returns.</summary>
    [ObservableProperty]
    private bool _hasLoaded;

    /// <summary>
    /// Whether the last fetch could not resolve the repo on GitHub (no GitHub remote,
    /// gh missing, or a failed fetch). Shows an honest "no GitHub data" note instead of
    /// the all-clear state, and keeps any previously loaded lists on screen.
    /// </summary>
    [ObservableProperty]
    private bool _isUnavailable;

    public GitHubDetailsViewModel(
        Repo repo,
        IGitHubService gitHubService,
        IProcessLauncher processLauncher)
    {
        _repo = repo;
        _gitHubService = gitHubService;
        _processLauncher = processLauncher;

        // Render from the column's last fetch when available, so opening the dialog is
        // instant; the fresh fetch below replaces the lists when it returns.
        var cached = gitHubService.GetCachedActivity(repo);
        if (cached is not null)
        {
            ApplyActivity(cached);
        }

        _ = RefreshAsync();
    }

    /// <summary>The repo name shown in the dialog title.</summary>
    public string RepoName => _repo.Name ?? string.Empty;

    /// <summary>The repo's GitHub HTML URL, or null for non-GitHub repos (hides the link).</summary>
    public string? RepoUrl => _repo.GitHubRepoUrl;

    /// <summary>Whether the pull-requests section shows (at least one open PR).</summary>
    public bool HasPullRequests => PullRequests.Count > 0;

    /// <summary>Whether the issues section shows (at least one open issue).</summary>
    public bool HasIssues => Issues.Count > 0;

    /// <summary>Whether the all-clear empty state shows: loaded, a GitHub repo, and
    /// nothing open at all.</summary>
    public bool HasNoItems
        => HasLoaded && !IsUnavailable && PullRequests.Count == 0 && Issues.Count == 0;

    /// <summary>Whether the "no GitHub data" note shows: loaded, unavailable, and no
    /// previously fetched lists to fall back on.</summary>
    public bool ShowUnavailableNote => HasLoaded && IsUnavailable && !HasPullRequests && !HasIssues;

    partial void OnPullRequestsChanged(ObservableCollection<GitHubItem> value)
    {
        OnPropertyChanged(nameof(HasPullRequests));
        OnPropertyChanged(nameof(HasNoItems));
        OnPropertyChanged(nameof(ShowUnavailableNote));
    }

    partial void OnIssuesChanged(ObservableCollection<GitHubItem> value)
    {
        OnPropertyChanged(nameof(HasIssues));
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
    /// Fetches the repo's open pull requests and issues fresh from <c>gh</c> and
    /// replaces both lists. Re-entrant clicks are coalesced via
    /// <see cref="CanRefresh"/>. Failures leave the current lists untouched (the
    /// service logs) — the dialog just stops spinning.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanRefresh))]
    private async Task RefreshAsync()
    {
        IsRefreshing = true;
        try
        {
            var activity = await _gitHubService.RefreshRepoAsync(_repo);
            // A failed fetch (no GitHub remote, gh missing, network down) returns an
            // empty activity — applying it would blank the lists and flash a misleading
            // all-clear. Keep the previous lists and surface the unavailable note.
            IsUnavailable = !_repo.GitHubAvailable;
            ApplyActivity(activity);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "GitHub details refresh failed for {FolderPath}", _repo.FolderPath);
        }
        finally
        {
            IsRefreshing = false;
        }
    }

    /// <summary>A refresh can start only when one isn't already running.</summary>
    private bool CanRefresh() => !IsRefreshing;

    /// <summary>Opens the clicked pull request / issue on github.com in the browser.</summary>
    [RelayCommand]
    private void OpenItem(GitHubItem? item)
    {
        if (!string.IsNullOrWhiteSpace(item?.Url))
        {
            _processLauncher.StartProcess(item.Url);
        }
    }

    /// <summary>Opens the repo's GitHub page in the browser.</summary>
    [RelayCommand]
    private void OpenRepo()
    {
        if (!string.IsNullOrWhiteSpace(RepoUrl))
        {
            _processLauncher.StartProcess(RepoUrl);
        }
    }

    private void ApplyActivity(GitHubActivity activity)
    {
        PullRequests = new ObservableCollection<GitHubItem>(activity.PullRequests);
        Issues = new ObservableCollection<GitHubItem>(activity.Issues);
        OnPropertyChanged(nameof(RepoUrl));
        HasLoaded = true;
    }
}
