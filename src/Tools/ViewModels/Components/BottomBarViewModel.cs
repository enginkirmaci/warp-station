using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Formatters;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.Services.Abstractions;

namespace Tools.ViewModels.Components;

/// <summary>The panels the bottom bar can expand to. <see cref="None"/> is the collapsed strip.</summary>
public enum BottomBarTab
{
    None = 0,
    Overview,
    Changes,
    PullRequests,
    Issues,
    Azure,
    Git,
    OpenCode,
}

/// <summary>
/// Binding adapter for the Repos page's bottom bar. Owns the bar's selected repo (the
/// git controls, GitHub/Azure panels and OpenCode launch all target it), the expandable
/// tab panels' data, and — relocated from the Repos page's overlay panel — the whole
/// OpenCode launch surface. Unlike the transient page ViewModels this one is a singleton:
/// it lives as long as the window, so the bar's state survives page navigation.
/// <para>
/// The bar stays hidden until a repo is selected from the table — a row press or any
/// row chip routing to a tab (constructor-injected reference; every row chip that used
/// to open a modal dialog routes to the matching tab, passing its row's repo).
/// </para>
/// </summary>
public partial class BottomBarViewModel : ObservableObject
{
    private readonly ISettingsService _settingsService;
    private readonly IRepoService _repoService;
    private readonly IGitStatusService _gitStatusService;
    private readonly IGitHubService _gitHubService;
    private readonly IAzureDevOpsService _azureDevOpsService;
    private readonly IProcessLauncher _processLauncher;
    private readonly IOpenCodeModelService _openCodeModelService;
    private readonly IOpenCodeTemplateService _openCodeTemplateService;
    private readonly IOpenCodePromptService _openCodePromptService;
    private readonly IOpenCodeGridLauncher _openCodeGridLauncher;
    private readonly INotificationService _notificationService;
    private readonly IClipboardService _clipboardService;

    private ReposSettings _reposSettings = new();
    private OpenCodeSettings _openCodeSettings = new();

    /// <summary>
    /// Guards the repo-dropdown rebuild posted from <see cref="IRepoService.Changed"/>:
    /// one scan raises Changed several times, and a single dispatcher pass per burst is enough.
    /// </summary>
    private bool _reposRebuildPosted;

    /// <summary>
    /// True while the branch ComboBox is being synced programmatically (repo switch,
    /// checkout completion) so the TwoWay selection change doesn't re-run a checkout.
    /// </summary>
    private bool _updatingBranchSelection;

    public BottomBarViewModel(
        ISettingsService settingsService,
        IRepoService repoService,
        IGitStatusService gitStatusService,
        IGitHubService gitHubService,
        IAzureDevOpsService azureDevOpsService,
        IProcessLauncher processLauncher,
        IOpenCodeModelService openCodeModelService,
        IOpenCodeTemplateService openCodeTemplateService,
        IOpenCodePromptService openCodePromptService,
        IOpenCodeGridLauncher openCodeGridLauncher,
        INotificationService notificationService,
        IClipboardService clipboardService)
    {
        _settingsService = settingsService;
        _repoService = repoService;
        _gitStatusService = gitStatusService;
        _gitHubService = gitHubService;
        _azureDevOpsService = azureDevOpsService;
        _processLauncher = processLauncher;
        _openCodeModelService = openCodeModelService;
        _openCodeTemplateService = openCodeTemplateService;
        _openCodePromptService = openCodePromptService;
        _openCodeGridLauncher = openCodeGridLauncher;
        _notificationService = notificationService;
        _clipboardService = clipboardService;

        _repoService.Changed += OnRepoServiceChanged;
        _ = InitializeAsync();
    }

    private async Task InitializeAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            _reposSettings = settings.Repos ?? new ReposSettings();
            _openCodeSettings = settings.OpenCode ?? new OpenCodeSettings();

            IsGitHubEnabled = _reposSettings.ShowGitHubColumn;
            IsAzureDevOpsEnabled = _reposSettings.ShowAzureDevOpsColumn;
            IsOpenCodeEnabled = _openCodeSettings.Enabled;
            RefreshOpenCodeAvailability();

            await LoadOpenCodeTemplatesAsync();
            await LoadOpenCodePromptsAsync();

            // Same idempotent load call the Repos page makes: the session's background
            // scan starts without waiting for a page visit. No repo is selected here —
            // the bar stays hidden until the user picks one from the table.
            await _repoService.EnsureLoadedAsync(_reposSettings);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Bottom bar initialization failed");
        }
    }

    // --- Repo context ---

    /// <summary>
    /// The repo the whole bar acts on: branch dropdown, fetch, changes list, GitHub /
    /// Azure panels and the OpenCode launch target. Set only from the table (row press
    /// or row chip); null until then, which keeps the bar hidden.
    /// </summary>
    [ObservableProperty]
    private Repo? _selectedRepo;

    public bool HasSelectedRepo => SelectedRepo is not null;

    public string SelectedRepoName => SelectedRepo?.Name ?? "No repository";

    partial void OnSelectedRepoChanged(Repo? value)
    {
        if (!ReferenceEquals(value, _observedRepo))
        {
            if (_observedRepo is not null)
            {
                _observedRepo.PropertyChanged -= OnSelectedRepoPropertyChanged;
                _observedRepo.IsBarSelected = false;
            }
            _observedRepo = value;
            if (_observedRepo is not null)
            {
                _observedRepo.PropertyChanged += OnSelectedRepoPropertyChanged;
                _observedRepo.IsBarSelected = true;
            }

            // Fresh repo: reload everything the open panel shows, plus the branch list.
            _ = LoadBranchesAsync();
            ReloadActiveTab();

            // CanExecute inputs the generator cannot hook (computed, not ObservableProperty).
            ResetOpenCodeTemplateCommand.NotifyCanExecuteChanged();
            FetchCommand.NotifyCanExecuteChanged();
            LaunchOpenCodeCommand.NotifyCanExecuteChanged();
        }

        OnPropertyChanged(nameof(HasSelectedRepo));
        OnPropertyChanged(nameof(SelectedRepoName));
        RaiseRepoDerived();
    }

    /// <summary>The repo currently subscribed for badge/label forwarding. Never bound.</summary>
    private Repo? _observedRepo;

    /// <summary>
    /// Forwards the selected repo's live git/GitHub counters onto the bar's computed
    /// bindings (tab badges, changes chip, last-fetched label). The git status service
    /// pushes these from background threads; Avalonia marshals the binding updates.
    /// </summary>
    private void OnSelectedRepoPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(Repo.GitModifiedCount)
            or nameof(Repo.GitHubPrCount)
            or nameof(Repo.GitHubIssueCount)
            or nameof(Repo.GitLastFetchAt)
            or nameof(Repo.GitBranchName)
            or nameof(Repo.GitHubRepoUrl))
        {
            RaiseRepoDerived();
        }
    }

    /// <summary>Raises every computed property derived from <see cref="SelectedRepo"/>.</summary>
    private void RaiseRepoDerived()
    {
        OnPropertyChanged(nameof(ChangesCount));
        OnPropertyChanged(nameof(ShowChangesBadge));
        OnPropertyChanged(nameof(ChangesChipText));
        OnPropertyChanged(nameof(PullRequestCount));
        OnPropertyChanged(nameof(ShowPullRequestBadge));
        OnPropertyChanged(nameof(IssueCount));
        OnPropertyChanged(nameof(ShowIssueBadge));
        OnPropertyChanged(nameof(LastFetchText));
        OnPropertyChanged(nameof(HasFetched));
        OnPropertyChanged(nameof(SelectedRepoBranch));
        OnPropertyChanged(nameof(HasSelectedRepoGitHubUrl));
        OnPropertyChanged(nameof(SelectedRepoGitHubDisplayUrl));
    }

    /// <summary>Working-tree change count of the selected repo (the Changes badge/chip).</summary>
    public int ChangesCount => SelectedRepo?.GitModifiedCount ?? 0;

    public bool ShowChangesBadge => ChangesCount > 0;

    /// <summary>"5 changes" label for the strip chip (singular-aware).</summary>
    public string ChangesChipText => ChangesCount == 1 ? "1 change" : $"{ChangesCount} changes";

    public int PullRequestCount => SelectedRepo?.GitHubPrCount ?? 0;

    public bool ShowPullRequestBadge => PullRequestCount > 0;

    public int IssueCount => SelectedRepo?.GitHubIssueCount ?? 0;

    public bool ShowIssueBadge => IssueCount > 0;

    /// <summary>"Last fetched: 2m ago", or null when the repo was never fetched.</summary>
    public string? LastFetchText => SelectedRepo?.GitLastFetchLabel is { } label ? $"Last fetched: {label}" : null;

    public bool HasFetched => SelectedRepo?.GitLastFetchAt is not null;

    // --- Repo header (the page's title while a repo is selected) ---

    /// <summary>Current branch of the selected repo (the header's branch line).</summary>
    public string? SelectedRepoBranch => SelectedRepo?.GitBranchName;

    /// <summary>Whether the header can offer GitHub entry points for the selected repo.</summary>
    public bool HasSelectedRepoGitHubUrl => !string.IsNullOrWhiteSpace(SelectedRepo?.GitHubRepoUrl);

    /// <summary>
    /// The GitHub URL in display form — scheme stripped, so <c>github.com/owner/repo</c>.
    /// </summary>
    public string? SelectedRepoGitHubDisplayUrl
    {
        get
        {
            if (SelectedRepo?.GitHubRepoUrl is not { } url) return null;
            if (Uri.TryCreate(url, UriKind.Absolute, out var uri))
            {
                var path = uri.PathAndQuery.TrimEnd('/');
                return string.IsNullOrEmpty(path) ? uri.Host : uri.Host + path;
            }

            return url;
        }
    }

    /// <summary>Toggles the selected repo's favorite (the header's star) via the repo service.</summary>
    [RelayCommand]
    private async Task ToggleSelectedRepoFavorite()
    {
        if (SelectedRepo is { } repo)
        {
            await _repoService.ToggleFavoriteAsync(repo);
        }
    }

    /// <summary>Copies the selected repo's GitHub URL (header kebab menu).</summary>
    [RelayCommand]
    private void CopyGitHubUrl()
    {
        if (SelectedRepo?.GitHubRepoUrl is { } url)
        {
            _clipboardService.CopyText(url);
            _notificationService.Show("GitHub URL copied", NotificationKind.Success);
        }
    }

    /// <summary>Copies the selected repo's folder path (header kebab menu).</summary>
    [RelayCommand]
    private void CopyRepoPath()
    {
        if (SelectedRepo?.FolderPath is { } path)
        {
            _clipboardService.CopyText(path);
            _notificationService.Show("Folder path copied", NotificationKind.Success);
        }
    }

    /// <summary>
    /// The header's X button (far right): closes the panel, leaving the strip visible.
    /// A row press (Overview) or any row chip reopens it.
    /// </summary>
    [RelayCommand]
    private void Close() => ActiveTab = BottomBarTab.None;

    private void OnRepoServiceChanged(object? sender, EventArgs e)
    {
        // Changed fires on background threads (scan completion); re-resolve on the UI
        // thread, once per burst.
        if (_reposRebuildPosted) return;
        _reposRebuildPosted = true;
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            _reposRebuildPosted = false;
            SyncSelectedRepoInstance();
        });
    }

    /// <summary>
    /// A rescan replaces the repo entities; if the bar's repo was one of them, re-resolve
    /// it by folder path so the bar keeps tracking the live entity. Never auto-picks a
    /// repo — the bar only ever acts on one the user selected from the table.
    /// </summary>
    private void SyncSelectedRepoInstance()
    {
        if (SelectedRepo is not { } selected) return;
        var current = _repoService.Repos;
        if (current.Any(r => ReferenceEquals(r, selected))) return;

        var path = selected.FolderPath;
        var match = path is null
            ? null
            : current.FirstOrDefault(r => RepoPath.SamePath(r.FolderPath ?? string.Empty, path));
        if (match is not null)
        {
            SelectedRepo = match;
        }
    }

    /// <summary>
    /// Applies freshly saved Repos settings (called by the Repos page after its settings
    /// dialog confirms): the GitHub/Azure tab visibility flags and the OpenCode
    /// availability (the executable lives in Repos settings) re-resolve immediately.
    /// </summary>
    public void ApplySettings(ReposSettings edited)
    {
        _reposSettings = edited;
        IsGitHubEnabled = edited.ShowGitHubColumn;
        IsAzureDevOpsEnabled = edited.ShowAzureDevOpsColumn;
        RefreshOpenCodeAvailability();
    }

    // --- Visibility ---

    /// <summary>
    /// Whether the bar shows at all. Hidden until a repo is selected from the table — a
    /// row press (<see cref="OpenForRepo"/>) or any row chip routing to a tab — so the
    /// page loads without the bar and the user's first pick reveals it.
    /// </summary>
    [ObservableProperty]
    private bool _isBarVisible;

    /// <summary>Selects a repo from a table row press and reveals the bar. The panel
    /// always opens on Overview — the repo view of the mockup — regardless of which tab
    /// was open before (row chips still route to their own tabs).</summary>
    public void OpenForRepo(Repo repo)
    {
        SetTargetRepo(repo);
        IsBarVisible = true;
        ActiveTab = BottomBarTab.Overview;
        _ = LoadOverviewAsync();
    }

    // --- Tabs ---

    [ObservableProperty]
    private BottomBarTab _activeTab;

    public bool IsPanelOpen => ActiveTab != BottomBarTab.None;

    /// <summary>
    /// The expanded panel's height, identical for every tab (the header plus the tab's
    /// content — sized so the Overview's five-row lists fit without inner scrolling;
    /// longer tab lists scroll internally).
    /// </summary>
    public double PanelHeight => 440d;

    public bool IsActiveOverview => ActiveTab == BottomBarTab.Overview;
    public bool IsActiveChanges => ActiveTab == BottomBarTab.Changes;
    public bool IsActivePullRequests => ActiveTab == BottomBarTab.PullRequests;
    public bool IsActiveIssues => ActiveTab == BottomBarTab.Issues;
    public bool IsActiveAzure => ActiveTab == BottomBarTab.Azure;
    public bool IsActiveGit => ActiveTab == BottomBarTab.Git;
    public bool IsActiveOpenCode => ActiveTab == BottomBarTab.OpenCode;

    partial void OnActiveTabChanged(BottomBarTab value)
    {
        OnPropertyChanged(nameof(IsPanelOpen));
        OnPropertyChanged(nameof(PanelHeight));
        OnPropertyChanged(nameof(IsActiveOverview));
        OnPropertyChanged(nameof(IsActiveChanges));
        OnPropertyChanged(nameof(IsActivePullRequests));
        OnPropertyChanged(nameof(IsActiveIssues));
        OnPropertyChanged(nameof(IsActiveAzure));
        OnPropertyChanged(nameof(IsActiveGit));
        OnPropertyChanged(nameof(IsActiveOpenCode));
    }

    /// <summary>
    /// Raised when the OpenCode enabled flag or default model changes here, so the Repos
    /// page (whose per-row buttons gate on its own snapshot) can refresh its availability
    /// flags. The page subscribes on construction and unsubscribes on navigate-from —
    /// symmetric lifetimes, no leak.
    /// </summary>
    public event Action? OpenCodeStateChanged;

    /// <summary>
    /// Points the bar at a repo chosen from the table (row chip or row press); the first
    /// pick reveals the bar. Switching repos leaves the open panel alone —
    /// <see cref="OnSelectedRepoChanged"/> reloads its data for the new target.
    /// </summary>
    private void SetTargetRepo(Repo? repo)
    {
        if (repo is null) return;
        IsBarVisible = true;
        if (!ReferenceEquals(repo, SelectedRepo))
        {
            SelectedRepo = repo;
        }
    }

    /// <summary>Reloads whichever panel is open after the target repo changed underneath it.</summary>
    private void ReloadActiveTab()
    {
        switch (ActiveTab)
        {
            case BottomBarTab.Overview:
                _ = LoadOverviewAsync();
                break;
            case BottomBarTab.Changes:
                _ = LoadChangedFilesAsync();
                break;
            case BottomBarTab.PullRequests:
            case BottomBarTab.Issues:
                _ = LoadGitHubAsync();
                break;
            case BottomBarTab.Azure:
                _ = LoadAzureAsync();
                break;
            case BottomBarTab.Git:
                _ = LoadGitAsync();
                break;
        }
    }

    // --- Git: branch dropdown, checkout, fetch ---

    /// <summary>Local branches of the selected repo for the branch dropdown.</summary>
    [ObservableProperty]
    private ObservableCollection<string> _branches = new();

    [ObservableProperty]
    private string? _selectedBranch;

    /// <summary>True while a checkout is running; disables the branch dropdown.</summary>
    [ObservableProperty]
    private bool _isCheckingOut;

    /// <summary>True while a fetch is running; disables the Fetch button.</summary>
    [ObservableProperty]
    private bool _isFetching;

    /// <summary>
    /// The branch dropdown is active: picking a branch checks it out in the selected
    /// repo. Programmatic syncs (repo switch, checkout completion) pass through the
    /// <see cref="_updatingBranchSelection"/> guard; a failed checkout reverts the
    /// dropdown to the repo's actual branch.
    /// </summary>
    partial void OnSelectedBranchChanged(string? value)
    {
        if (_updatingBranchSelection || IsCheckingOut) return;
        if (string.IsNullOrWhiteSpace(value) || SelectedRepo is null) return;
        if (string.Equals(value, SelectedRepo.GitBranchName, StringComparison.Ordinal)) return;
        _ = CheckoutAsync(value);
    }

    private async Task CheckoutAsync(string branch)
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        IsCheckingOut = true;
        try
        {
            if (await _gitStatusService.CheckoutAsync(repo, branch))
            {
                _notificationService.Show($"Checked out {branch} in {repo.Name}", NotificationKind.Success);
                SyncBranchSelection(repo);
                _ = LoadBranchesAsync();
            }
            else
            {
                _notificationService.Show($"Checkout of {branch} failed", NotificationKind.Error);
                SyncBranchSelection(repo);
            }
        }
        finally
        {
            IsCheckingOut = false;
        }
    }

    /// <summary>
    /// Loads the branch list and syncs the dropdown to the repo's current branch. The
    /// repo entity's branch updates asynchronously (the status refresh inside the
    /// checkout/fetch completes later), so re-sync once more when it lands.
    /// </summary>
    private async Task LoadBranchesAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            Branches.Clear();
            SyncBranchSelection(null);
            return;
        }

        try
        {
            var branches = await _gitStatusService.GetBranchesAsync(repo);
            if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading

            // Clear() resets the ComboBox's selection, and re-assigning an unchanged
            // SelectedBranch value afterwards raises no change — the placeholder would
            // stick. Rebuild only when the list really changed, and drop the stale
            // selection first (guarded: the null must not read as a user checkout pick).
            if (Branches.Count != branches.Count || !Branches.SequenceEqual(branches))
            {
                Branches.Clear();
                foreach (var branch in branches)
                {
                    Branches.Add(branch);
                }
                _updatingBranchSelection = true;
                try
                {
                    SelectedBranch = null;
                }
                finally
                {
                    _updatingBranchSelection = false;
                }
            }
            SyncBranchSelection(repo);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Bottom bar branch load failed for {Path}", repo.FolderPath);
        }
    }

    private void SyncBranchSelection(Repo? repo)
    {
        _updatingBranchSelection = true;
        try
        {
            SelectedBranch = repo?.GitBranchName;
        }
        finally
        {
            _updatingBranchSelection = false;
        }
    }

    [RelayCommand(CanExecute = nameof(CanFetch))]
    private async Task FetchAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        IsFetching = true;
        try
        {
            if (await _gitStatusService.FetchAsync(repo))
            {
                _notificationService.Show($"Fetched {repo.Name}", NotificationKind.Success);
                SyncBranchSelection(repo);
            }
            else
            {
                _notificationService.Show($"Fetch failed for {repo.Name}", NotificationKind.Error);
            }
        }
        finally
        {
            IsFetching = false;
        }
    }

    private bool CanFetch() => !IsFetching && HasSelectedRepo;

    // --- Changes tab ---

    [ObservableProperty]
    private ObservableCollection<GitChangedFile> _changedFiles = new();

    /// <summary>True while the change list is loading; gates the empty state.</summary>
    [ObservableProperty]
    private bool _isLoadingFiles;

    /// <summary>Working-tree line counts summed over the changed files (+X −Y footer).</summary>
    [ObservableProperty]
    private int _changesAdditions;

    [ObservableProperty]
    private int _changesDeletions;

    /// <summary>The "+124 −38" footer text; empty when no file carries line counts.</summary>
    public string ChangesDeltaText => ChangesAdditions == 0 && ChangesDeletions == 0
        ? string.Empty
        : $"+{ChangesAdditions} −{ChangesDeletions}";

    public bool ShowChangesEmpty => !IsLoadingFiles && ChangedFiles.Count == 0;

    /// <summary>First five changed files for the Overview card (the Changes tab lists all).</summary>
    public IEnumerable<GitChangedFile> ChangedFilesPreview => ChangedFiles.Take(5);

    partial void OnIsLoadingFilesChanged(bool value) => OnPropertyChanged(nameof(ShowChangesEmpty));

    partial void OnChangesAdditionsChanged(int value) => OnPropertyChanged(nameof(ChangesDeltaText));

    partial void OnChangesDeletionsChanged(int value) => OnPropertyChanged(nameof(ChangesDeltaText));

    private async Task LoadChangedFilesAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            ChangedFiles.Clear();
            ChangesAdditions = 0;
            ChangesDeletions = 0;
            OnPropertyChanged(nameof(ShowChangesEmpty));
            OnPropertyChanged(nameof(ChangedFilesPreview));
            return;
        }

        IsLoadingFiles = true;
        try
        {
            var files = await _gitStatusService.GetChangedFilesAsync(repo);
            if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading
            // Sum locally and assign: the observable totals are never reset between
            // loads, so accumulating on them would compound with every reload.
            var additions = 0;
            var deletions = 0;
            ChangedFiles.Clear();
            foreach (var file in files)
            {
                ChangedFiles.Add(file);
                additions += file.Additions ?? 0;
                deletions += file.Deletions ?? 0;
            }
            ChangesAdditions = additions;
            ChangesDeletions = deletions;
        }
        finally
        {
            IsLoadingFiles = false;
            OnPropertyChanged(nameof(ShowChangesEmpty));
            OnPropertyChanged(nameof(ChangedFilesPreview));
        }
    }

    // --- Overview tab ---

    /// <summary>Static GitHub metadata for the sidebar (owner, created, language, …).</summary>
    [ObservableProperty]
    private GitHubRepoDetails? _selectedRepoDetails;

    /// <summary>Whether the details sidebar has anything to show.</summary>
    public bool HasRepoDetails => SelectedRepoDetails is { HasContent: true };

    partial void OnSelectedRepoDetailsChanged(GitHubRepoDetails? value)
        => OnPropertyChanged(nameof(HasRepoDetails));

    /// <summary>
    /// Pipeline health line for the Pipelines card: the latest verdict across the
    /// selected repo's recent Azure DevOps runs.
    /// </summary>
    public string PipelineStatusText => AzurePipelineRuns.Count == 0 ? "No pipeline runs"
        : AzurePipelineRuns.Any(r => r.IsFailed) ? "Checks failing"
        : AzurePipelineRuns.Any(r => r.IsRunning) ? "Pipelines running"
        : "All checks passing";

    /// <summary>Whether the Pipelines card shows a failing state (drives its color).</summary>
    public bool IsPipelineFailing => AzurePipelineRuns.Any(r => r.IsFailed);

    /// <summary>Whether the Pipelines card shows a running state.</summary>
    public bool IsPipelineRunning => !IsPipelineFailing && AzurePipelineRuns.Any(r => r.IsRunning);

    /// <summary>Whether the Pipelines card shows a passing state.</summary>
    public bool IsPipelinePassing => !IsPipelineFailing && !IsPipelineRunning && AzurePipelineRuns.Count > 0;

    partial void OnAzurePipelineRunsChanged(ObservableCollection<AzureDevOpsPipelineRun> value)
    {
        RaisePipelineStatus();
    }

    private void RaisePipelineStatus()
    {
        OnPropertyChanged(nameof(PipelineStatusText));
        OnPropertyChanged(nameof(IsPipelineFailing));
        OnPropertyChanged(nameof(IsPipelineRunning));
        OnPropertyChanged(nameof(IsPipelinePassing));
    }

    /// <summary>
    /// Loads everything the Overview tab shows: the working-tree change list, the
    /// GitHub activity and Azure runs (each guarded by its own availability flag) and
    /// the static repository details. The list loads reuse the tabs' collections, so a
    /// switch from Overview to a tab shows instantly-populated data.
    /// </summary>
    private async Task LoadOverviewAsync()
    {
        _ = LoadChangedFilesAsync();
        if (IsGitHubEnabled)
        {
            _ = LoadGitHubAsync();
        }
        if (IsAzureDevOpsEnabled)
        {
            _ = LoadAzureAsync();
        }
        await LoadRepoDetailsAsync();
    }

    private async Task LoadRepoDetailsAsync()
    {
        var repo = SelectedRepo;
        if (repo is null || !IsGitHubEnabled)
        {
            SelectedRepoDetails = null;
            return;
        }

        var details = await _gitHubService.GetRepoDetailsAsync(repo);
        if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading
        SelectedRepoDetails = details;
    }

    // --- Git tab ---

    /// <summary>The selected repo's recent commits, newest first.</summary>
    [ObservableProperty]
    private ObservableCollection<GitCommitInfo> _gitCommits = new();

    /// <summary>True while the commit list is loading; gates the empty state.</summary>
    [ObservableProperty]
    private bool _isLoadingCommits;

    public bool ShowCommitsEmpty => !IsLoadingCommits && GitCommits.Count == 0;

    partial void OnIsLoadingCommitsChanged(bool value) => OnPropertyChanged(nameof(ShowCommitsEmpty));

    /// <summary>Loads the Git tab's data: the branch list plus the recent commit list.</summary>
    private async Task LoadGitAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            GitCommits.Clear();
            OnPropertyChanged(nameof(ShowCommitsEmpty));
            return;
        }

        IsLoadingCommits = true;
        try
        {
            var commits = await _gitStatusService.GetRecentCommitsAsync(repo);
            if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading
            GitCommits.Clear();
            foreach (var commit in commits)
            {
                GitCommits.Add(commit);
            }
        }
        finally
        {
            IsLoadingCommits = false;
            OnPropertyChanged(nameof(ShowCommitsEmpty));
        }
        _ = LoadBranchesAsync();
    }

    /// <summary>Checks a branch out from the Git tab's branch list (same path as the dropdown).</summary>
    [RelayCommand]
    private async Task CheckoutBranch(string? branch)
    {
        if (string.IsNullOrWhiteSpace(branch)) return;
        await CheckoutAsync(branch);
        _ = LoadGitAsync();
    }

    // --- GitHub tabs (pull requests + issues) ---

    /// <summary>Whether the GitHub tabs show at all (mirrors the GitHub column setting).</summary>
    [ObservableProperty]
    private bool _isGitHubEnabled;

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _gitHubPullRequests = new();

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _gitHubIssues = new();

    /// <summary>First five pull requests / issues for the Overview cards (tabs list all).</summary>
    public IEnumerable<GitHubItem> GitHubPullRequestsPreview => GitHubPullRequests.Take(5);

    public IEnumerable<GitHubItem> GitHubIssuesPreview => GitHubIssues.Take(5);

    partial void OnGitHubPullRequestsChanged(ObservableCollection<GitHubItem> value)
        => OnPropertyChanged(nameof(GitHubPullRequestsPreview));

    partial void OnGitHubIssuesChanged(ObservableCollection<GitHubItem> value)
        => OnPropertyChanged(nameof(GitHubIssuesPreview));

    [ObservableProperty]
    private bool _isGitHubRefreshing;

    [ObservableProperty]
    private bool _gitHubHasLoaded;

    [ObservableProperty]
    private bool _gitHubIsUnavailable;

    public bool ShowPullRequestsEmpty => GitHubHasLoaded && !GitHubIsUnavailable && GitHubPullRequests.Count == 0;
    public bool ShowIssuesEmpty => GitHubHasLoaded && !GitHubIsUnavailable && GitHubIssues.Count == 0;
    public bool ShowGitHubUnavailable => GitHubHasLoaded && GitHubIsUnavailable;

    private async Task LoadGitHubAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            GitHubPullRequests.Clear();
            GitHubIssues.Clear();
            return;
        }

        // Seed from the service cache so opening the tab is instant; the fresh fetch
        // below replaces the lists when it returns.
        var cached = _gitHubService.GetCachedActivity(repo);
        if (cached is not null)
        {
            ApplyGitHubActivity(cached);
        }

        await RefreshGitHubAsync();
    }

    [RelayCommand(CanExecute = nameof(CanRefreshGitHub))]
    private async Task RefreshGitHubAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        IsGitHubRefreshing = true;
        try
        {
            var activity = await _gitHubService.RefreshRepoAsync(repo);
            if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading
            // A failed fetch returns an empty activity — applying it would flash a
            // misleading all-clear. Keep any previous lists and surface the unavailable note.
            GitHubIsUnavailable = !repo.GitHubAvailable;
            ApplyGitHubActivity(activity);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "GitHub panel refresh failed for {FolderPath}", repo.FolderPath);
        }
        finally
        {
            IsGitHubRefreshing = false;
        }
    }

    private bool CanRefreshGitHub() => !IsGitHubRefreshing;

    private void ApplyGitHubActivity(GitHubActivity activity)
    {
        GitHubPullRequests = new ObservableCollection<GitHubItem>(activity.PullRequests);
        GitHubIssues = new ObservableCollection<GitHubItem>(activity.Issues);
        GitHubHasLoaded = true;
        OnPropertyChanged(nameof(ShowPullRequestsEmpty));
        OnPropertyChanged(nameof(ShowIssuesEmpty));
        OnPropertyChanged(nameof(ShowGitHubUnavailable));
    }

    /// <summary>Opens the clicked pull request / issue on github.com.</summary>
    [RelayCommand]
    private void OpenGitHubItem(GitHubItem? item)
    {
        if (!string.IsNullOrWhiteSpace(item?.Url))
        {
            _processLauncher.StartProcess(item.Url);
        }
    }

    /// <summary>Opens the selected repo's GitHub page in the browser.</summary>
    [RelayCommand]
    private void OpenGitHubRepo()
    {
        if (!string.IsNullOrWhiteSpace(SelectedRepo?.GitHubRepoUrl))
        {
            _processLauncher.StartProcess(SelectedRepo.GitHubRepoUrl);
        }
    }

    // --- Azure tab ---

    /// <summary>Whether the Azure tab shows at all (mirrors the Azure column setting).</summary>
    [ObservableProperty]
    private bool _isAzureDevOpsEnabled;

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
    public bool ShowAzureEmpty => AzureHasLoaded && !AzureIsUnavailable
        && AzurePullRequests.Count == 0 && AzureWorkItems.Count == 0 && AzurePipelineRuns.Count == 0;
    public bool ShowAzureUnavailable => AzureHasLoaded && AzureIsUnavailable
        && !HasAzurePullRequests && !HasAzureWorkItems && !HasAzurePipelineRuns;

    private async Task LoadAzureAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            AzurePullRequests.Clear();
            AzureWorkItems.Clear();
            AzurePipelineRuns.Clear();
            RaisePipelineStatus();
            return;
        }

        var cached = _azureDevOpsService.GetCachedActivity(repo);
        if (cached is not null)
        {
            ApplyAzureActivity(cached);
        }

        await RefreshAzureAsync();
    }

    [RelayCommand(CanExecute = nameof(CanRefreshAzure))]
    private async Task RefreshAzureAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        IsAzureRefreshing = true;
        try
        {
            var activity = await _azureDevOpsService.RefreshRepoAsync(repo);
            if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading
            AzureIsUnavailable = !repo.AzureDevOpsAvailable;
            ApplyAzureActivity(activity);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Azure panel refresh failed for {FolderPath}", repo.FolderPath);
        }
        finally
        {
            IsAzureRefreshing = false;
        }
    }

    private bool CanRefreshAzure() => !IsAzureRefreshing;

    private void ApplyAzureActivity(AzureDevOpsActivity activity)
    {
        AzurePullRequests = new ObservableCollection<AzureDevOpsItem>(activity.PullRequests);
        AzureWorkItems = new ObservableCollection<AzureDevOpsItem>(activity.WorkItems);
        AzurePipelineRuns = new ObservableCollection<AzureDevOpsPipelineRun>(activity.PipelineRuns);
        AzureHasLoaded = true;
        OnPropertyChanged(nameof(HasAzurePullRequests));
        OnPropertyChanged(nameof(HasAzureWorkItems));
        OnPropertyChanged(nameof(HasAzurePipelineRuns));
        OnPropertyChanged(nameof(ShowAzureEmpty));
        OnPropertyChanged(nameof(ShowAzureUnavailable));
        RaisePipelineStatus();
    }

    /// <summary>Opens the clicked pull request / work item in the browser.</summary>
    [RelayCommand]
    private void OpenAzureItem(AzureDevOpsItem? item)
    {
        if (!string.IsNullOrWhiteSpace(item?.Url))
        {
            _processLauncher.StartProcess(item.Url);
        }
    }

    /// <summary>Opens the clicked pipeline run in the browser.</summary>
    [RelayCommand]
    private void OpenAzurePipeline(AzureDevOpsPipelineRun? run)
    {
        if (!string.IsNullOrWhiteSpace(run?.Url))
        {
            _processLauncher.StartProcess(run.Url);
        }
    }

    /// <summary>Opens the selected repo's Azure DevOps page in the browser.</summary>
    [RelayCommand]
    private void OpenAzureRepo()
    {
        if (!string.IsNullOrWhiteSpace(SelectedRepo?.AzureDevOpsRepoUrl))
        {
            _processLauncher.StartProcess(SelectedRepo.AzureDevOpsRepoUrl);
        }
    }

    // --- OpenCode tab (relocated from the Repos page overlay panel) ---

    /// <summary>
    /// Whether the OpenCode integration is enabled (mirrors and persists
    /// <see cref="OpenCodeSettings.Enabled"/> — previously settings.json-only). Toggling
    /// it here is live: the per-row buttons re-evaluate via <see cref="OpenCodeStateChanged"/>.
    /// </summary>
    [ObservableProperty]
    private bool _isOpenCodeEnabled;

    /// <summary>
    /// Whether the OpenCode launch UI is usable: the integration enabled <em>and</em> the
    /// configured opencode CLI resolvable on this machine (see <see cref="ExecutableDefaults"/>).
    /// </summary>
    [ObservableProperty]
    private bool _hasOpenCode;

    /// <summary>The configured default model, exposed for the Repos page's quick-open launch.</summary>
    public string? OpenCodeDefaultModel => _openCodeSettings.DefaultModel;

    partial void OnIsOpenCodeEnabledChanged(bool value)
    {
        _openCodeSettings.Enabled = value;
        RefreshOpenCodeAvailability();
        _ = PersistOpenCodeSettingAsync(s => s.OpenCode.Enabled = value);
        OpenCodeStateChanged?.Invoke();
    }

    private void RefreshOpenCodeAvailability()
    {
        HasOpenCode = IsOpenCodeEnabled && ExecutableDefaults.Locate(_reposSettings.OpenCodeExecutable) is not null;
    }

    /// <summary>
    /// The models available in the OpenCode model selector, fetched by running
    /// <c>opencode models</c> as a one-shot process. (Re)populated each time the OpenCode
    /// tab opens; empty when the CLI fails or is missing.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<string> _openCodeModels = new();

    /// <summary>
    /// The currently selected model. Bound OneWay to the tab's model picker so the box
    /// genuinely selects (highlights) the configured default; user picks are committed by
    /// the bar code-behind's SelectionChanged handler (see <see cref="CommitOpenCodeModel"/>),
    /// not by a TwoWay binding — a TwoWay writeback would null the selection during the
    /// in-place list rebuilds (see <see cref="RefreshOpenCodeFilteredModels"/>).
    /// </summary>
    [ObservableProperty]
    private string _openCodeSelectedModel = string.Empty;

    /// <summary>
    /// The text the user is typing into the editable model ComboBox (the live search
    /// term, kept separate from the committed selection).
    /// </summary>
    [ObservableProperty]
    private string _openCodeModelFilter = string.Empty;

    /// <summary>
    /// The model list shown in the dropdown: <see cref="OpenCodeModels"/> filtered by
    /// <see cref="OpenCodeModelFilter"/> (case-insensitive <c>Contains</c>).
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<string> _openCodeFilteredModels = new();

    [ObservableProperty]
    private ObservableCollection<OpenCodeTemplate> _openCodeTemplates = new() { OpenCodeTemplate.None };

    [ObservableProperty]
    private OpenCodeTemplate _openCodeSelectedTemplate = OpenCodeTemplate.None;

    public string OpenCodeSelectedTemplateDescription => OpenCodeSelectedTemplate?.Description ?? string.Empty;

    [ObservableProperty]
    private ObservableCollection<OpenCodePromptEntry> _openCodePrompts = new() { OpenCodePromptEntry.None };

    [ObservableProperty]
    private OpenCodePromptEntry _openCodeSelectedPrompt = OpenCodePromptEntry.None;

    [ObservableProperty]
    private string _openCodePrompt = string.Empty;

    [ObservableProperty]
    private string _newPromptName = string.Empty;

    [ObservableProperty]
    private int _openCodeInstanceCount = 1;

    /// <summary>
    /// Whether to tile the launched opencode instances across the screen in a grid.
    /// Off by default — instances open as plain terminal windows; checking it routes the
    /// launch through <see cref="IOpenCodeGridLauncher"/>.
    /// </summary>
    [ObservableProperty]
    private bool _openCodeArrangeIntoGrid;

    /// <summary>
    /// Whether the OpenCode tab offers the "Arrange into grid" checkbox. The grid launcher
    /// positions windows through SnapIt's Win32 primitives, so the option only exists on
    /// Windows; the tab hides it elsewhere. Runtime check, never the build-OS constant.
    /// </summary>
    public bool CanArrangeIntoGrid => OperatingSystem.IsWindows();

    public bool OpenCodeHasModels => OpenCodeModels.Count > 0;
    public bool OpenCodeModelsEmpty => OpenCodeModels.Count == 0;

    /// <summary>
    /// Loads the model list: the cached list shows immediately, then <c>opencode models</c>
    /// runs and the fresh list replaces it. Called each time the OpenCode tab opens.
    /// </summary>
    private async Task LoadOpenCodeModelsAsync()
    {
        var cached = _openCodeModelService.GetCachedModels(_openCodeSettings.DefaultModel);
        if (cached.Count > 0)
            ApplyOpenCodeModels(cached);

        var models = await _openCodeModelService.GetModelsAsync(_reposSettings.OpenCodeExecutable, _openCodeSettings.DefaultModel);
        ApplyOpenCodeModels(models);
    }

    /// <summary>
    /// Pushes <paramref name="models"/> into <see cref="OpenCodeModels"/>, selects the
    /// configured default or first entry (or clears the selection when empty), and
    /// refreshes the filter projection and the computed has/empty flags. A model the user
    /// already picked survives the refresh when it is still present.
    /// </summary>
    private void ApplyOpenCodeModels(IReadOnlyList<string> models)
    {
        OpenCodeModels = new ObservableCollection<string>(models);

        var previous = OpenCodeSelectedModel;
        var previousStillListed = !string.IsNullOrWhiteSpace(previous) && OpenCodeModels.Contains(previous);
        OpenCodeSelectedModel = previousStillListed
            ? previous
            : SelectConfiguredOrDefaultModel(OpenCodeModels);

        OpenCodeModelFilter = OpenCodeSelectedModel;
        RefreshOpenCodeFilteredModels();

        // Re-raise so the OneWay SelectedItem binding re-resolves after the in-place list
        // rebuild — including when the value did not change and ObservableProperty raised
        // nothing. Safe from text clobbering: the filter was just mirrored to the same
        // value, and the code-behind's commit handler re-commits equal values (no loop).
        OnPropertyChanged(nameof(OpenCodeSelectedModel));

        OnPropertyChanged(nameof(OpenCodeHasModels));
        OnPropertyChanged(nameof(OpenCodeModelsEmpty));
    }

    /// <summary>
    /// The model to preselect (and launch) when the user has not picked one: the
    /// configured default when set and listed — matched case-insensitively and resolved
    /// to the list's own casing — otherwise the first model.
    /// </summary>
    private string SelectConfiguredOrDefaultModel(IReadOnlyList<string> models)
    {
        var configured = _openCodeSettings.DefaultModel?.Trim();
        if (!string.IsNullOrEmpty(configured))
        {
            var match = models.FirstOrDefault(m => string.Equals(m, configured, StringComparison.OrdinalIgnoreCase));
            if (match is not null)
                return match;
        }

        return models.FirstOrDefault() ?? string.Empty;
    }

    /// <summary>
    /// The model to launch with, in priority order: an exact match for what the box
    /// shows, the committed dropdown selection, and finally the configured default or
    /// the first model.
    /// </summary>
    private string ResolveOpenCodeLaunchModel()
    {
        var typed = OpenCodeModelFilter?.Trim();
        var typedMatch = string.IsNullOrWhiteSpace(typed)
            ? null
            : OpenCodeModels.FirstOrDefault(m => string.Equals(m, typed, StringComparison.OrdinalIgnoreCase));

        if (typedMatch is not null)
        {
            return typedMatch;
        }

        return string.IsNullOrWhiteSpace(OpenCodeSelectedModel)
            ? SelectConfiguredOrDefaultModel(OpenCodeModels)
            : OpenCodeSelectedModel;
    }

    /// <summary>
    /// Commits a model picked from the dropdown (called by the bar code-behind): updates
    /// the selection and filter, and PERSISTS the pick as the configured default model —
    /// the OpenCode tab is the settings surface for a value that previously could only be
    /// edited by hand in settings.json.
    /// </summary>
    public async Task CommitOpenCodeModelAsync(string model)
    {
        if (string.IsNullOrWhiteSpace(model)) return;
        if (string.Equals(model, OpenCodeSelectedModel, StringComparison.Ordinal)
            && string.Equals(_openCodeSettings.DefaultModel, model, StringComparison.Ordinal))
        {
            OpenCodeModelFilter = model;
            return;
        }

        OpenCodeSelectedModel = model;
        OpenCodeModelFilter = model;
        _openCodeSettings.DefaultModel = model;
        OnPropertyChanged(nameof(OpenCodeDefaultModel));

        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            settings.OpenCode ??= new OpenCodeSettings();
            settings.OpenCode.DefaultModel = model;
            await _settingsService.SaveSettingsAsync(settings);
            _notificationService.Show($"Default model set to {model}", NotificationKind.Success);
            OpenCodeStateChanged?.Invoke();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Failed to persist the OpenCode default model");
        }
    }

    /// <summary>
    /// Rebuilds <see cref="OpenCodeFilteredModels"/> from <see cref="OpenCodeModels"/>
    /// using the current filter. Must not run synchronously from a filter writeback that
    /// originates inside the ComboBox's own selection update — see
    /// <see cref="ScheduleFilteredModelsRefresh"/>.
    /// </summary>
    private void RefreshOpenCodeFilteredModels()
    {
        var filter = OpenCodeModelFilter ?? string.Empty;
        bool isFullSelection = string.IsNullOrEmpty(filter)
            || string.Equals(filter, OpenCodeSelectedModel, StringComparison.Ordinal);
        var source = (isFullSelection
            ? OpenCodeModels
            : OpenCodeModels.Where(m => m.Contains(filter, StringComparison.OrdinalIgnoreCase))).ToList();

        // Rebuild in place rather than swapping in a new instance: the ComboBox's Text
        // binding raises the filter change from inside the control's own selection
        // update, and re-sourcing ItemsSource there throws "Cannot change source while
        // update is in progress". Skip the rebuild entirely when the projection already
        // matches — Clear() raises a Reset which drops the control-side selection even
        // when the content is identical.
        if (source.Count == OpenCodeFilteredModels.Count && source.SequenceEqual(OpenCodeFilteredModels))
            return;

        OpenCodeFilteredModels.Clear();
        foreach (var model in source)
            OpenCodeFilteredModels.Add(model);
    }

    /// <summary>Whether a deferred <see cref="RefreshOpenCodeFilteredModels"/> pass is queued.</summary>
    private bool _filteredModelsRefreshScheduled;

    partial void OnOpenCodeModelFilterChanged(string value) => ScheduleFilteredModelsRefresh();

    /// <summary>
    /// Schedules <see cref="RefreshOpenCodeFilteredModels"/> on the next dispatcher pass,
    /// coalescing bursts into one rebuild. The deferral is load-bearing: mutating the
    /// filtered list synchronously from the Text writeback raises CollectionChanged
    /// re-entrantly inside the ComboBox's selection update and the selection model throws.
    /// </summary>
    private void ScheduleFilteredModelsRefresh()
    {
        if (_filteredModelsRefreshScheduled)
        {
            return;
        }

        _filteredModelsRefreshScheduled = true;
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            _filteredModelsRefreshScheduled = false;

            // Capture whether the box is supposed to be showing the committed selection
            // before rebuilding — while a user search is in flight the filter differs.
            bool boxShowsSelection = string.Equals(OpenCodeModelFilter, OpenCodeSelectedModel, StringComparison.Ordinal);
            RefreshOpenCodeFilteredModels();

            // A rebuild that actually runs drops the ComboBox's control-side selection;
            // when the box was showing the committed selection, re-push it so the OneWay
            // SelectedItem binding re-resolves and reselects the entry.
            if (boxShowsSelection && !string.IsNullOrEmpty(OpenCodeSelectedModel))
                OnPropertyChanged(nameof(OpenCodeSelectedModel));
        });
    }

    partial void OnOpenCodeModelsChanged(ObservableCollection<string> value)
        => ScheduleFilteredModelsRefresh();

    private async Task LoadOpenCodeTemplatesAsync()
    {
        var templates = await _openCodeTemplateService.LoadAsync();
        var collection = new ObservableCollection<OpenCodeTemplate> { OpenCodeTemplate.None };
        foreach (var template in templates)
            collection.Add(template);
        OpenCodeTemplates = collection;
    }

    private async Task LoadOpenCodePromptsAsync()
    {
        var prompts = await _openCodePromptService.LoadAsync();
        var collection = new ObservableCollection<OpenCodePromptEntry> { OpenCodePromptEntry.None };
        foreach (var prompt in prompts)
            collection.Add(prompt);
        OpenCodePrompts = collection;
    }

    partial void OnOpenCodeSelectedPromptChanged(OpenCodePromptEntry value)
    {
        if (value is null || value.IsNone)
            return;
        OpenCodePrompt = value.Prompt;
    }

    /// <summary>
    /// Saves the current Start prompt under the name in <see cref="NewPromptName"/>, reloads
    /// the selector and selects the saved entry.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanSavePrompt))]
    private async Task SavePromptAsync()
    {
        var name = (NewPromptName ?? string.Empty).Trim();
        if (string.IsNullOrEmpty(name)) return;

        await _openCodePromptService.SaveAsync(name, OpenCodePrompt ?? string.Empty);
        NewPromptName = string.Empty;

        await LoadOpenCodePromptsAsync();

        OpenCodeSelectedPrompt = OpenCodePrompts.FirstOrDefault(p =>
            string.Equals(p.Name, name, StringComparison.OrdinalIgnoreCase)) ?? OpenCodePromptEntry.None;
        _notificationService.Show("Prompt saved", NotificationKind.Success);
    }

    private bool CanSavePrompt()
        => !string.IsNullOrWhiteSpace(NewPromptName) && !string.IsNullOrWhiteSpace(OpenCodePrompt);

    partial void OnNewPromptNameChanged(string value) => SavePromptCommand.NotifyCanExecuteChanged();
    partial void OnOpenCodePromptChanged(string value) => SavePromptCommand.NotifyCanExecuteChanged();

    /// <summary>
    /// Removes the selected repo's <c>.opencode</c> folder and re-copies the currently
    /// selected template into it, without launching OpenCode.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanResetOpenCodeTemplate))]
    private async Task ResetOpenCodeTemplateAsync()
    {
        var repo = SelectedRepo;
        if (repo?.FolderPath is null || OpenCodeSelectedTemplate.IsNone)
            return;

        await _openCodeTemplateService.CopyToRepoAsync(OpenCodeSelectedTemplate, repo.FolderPath);
        _notificationService.Show("Template reset", NotificationKind.Success);
    }

    private bool CanResetOpenCodeTemplate()
        => SelectedRepo?.FolderPath is not null && !OpenCodeSelectedTemplate.IsNone;

    /// <summary>
    /// Re-evaluate <see cref="ResetOpenCodeTemplateCommand"/>, refresh the computed
    /// description binding, and coerce transient nulls (the ComboBox TwoWay binding pushes
    /// null when <see cref="OpenCodeTemplates"/> is swapped) back to the None sentinel.
    /// </summary>
    partial void OnOpenCodeSelectedTemplateChanged(OpenCodeTemplate value)
    {
        if (value is null)
        {
            OpenCodeSelectedTemplate = OpenCodeTemplate.None;
            return;
        }
        ResetOpenCodeTemplateCommand.NotifyCanExecuteChanged();
        OnPropertyChanged(nameof(OpenCodeSelectedTemplateDescription));
    }

    /// <summary>
    /// Launches opencode in the selected repo with the current tab options (model,
    /// instances, grid, template, prompt). Identical to the old panel launch, targeting
    /// the bar's repo; the tab closes once the instances are on their way.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanLaunchOpenCode))]
    private async Task LaunchOpenCodeAsync()
    {
        var repo = SelectedRepo;
        if (repo?.FolderPath is null || !HasOpenCode) return;

        // Copy the selected template (if any) to <repo>/.opencode before launching.
        await _openCodeTemplateService.CopyToRepoAsync(OpenCodeSelectedTemplate, repo.FolderPath);

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null)
        {
            ActiveTab = BottomBarTab.None;
            return;
        }

        var openCodeExe = ResolveCliForTerminal(_reposSettings.OpenCodeExecutable, "opencode");
        var prompt = OpenCodePrompt?.Trim();
        var count = OpenCodeInstanceCount < 1 ? 1 : OpenCodeInstanceCount;
        var model = ResolveOpenCodeLaunchModel();

        if (OpenCodeArrangeIntoGrid)
        {
            await _openCodeGridLauncher.LaunchAsync(terminalExe, openCodeExe, repo.FolderPath, model, prompt ?? string.Empty, count);
        }
        else
        {
            var commandLine = OpenCodeGridLauncher.BuildCommandLine(openCodeExe, model, prompt ?? string.Empty);
            var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, commandLine);
            for (var i = 0; i < count; i++)
            {
                _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
            }
        }

        ActiveTab = BottomBarTab.None;
    }

    private bool CanLaunchOpenCode() => HasOpenCode && HasSelectedRepo;

    /// <summary>
    /// Resolves a CLI name for embedding in a terminal command line: the spawned terminal
    /// inherits the app's often-minimal GUI PATH, so a bare name is expanded to its
    /// absolute path; when unresolvable the bare name is kept so the terminal shows the
    /// familiar "command not found" feedback.
    /// </summary>
    private static string ResolveCliForTerminal(string? configured, string fallback)
    {
        var resolved = ExecutableDefaults.Locate(configured) ?? configured ?? fallback;
        return resolved.Contains(' ') ? $"\"{resolved}\"" : resolved;
    }

    private async Task PersistOpenCodeSettingAsync(Action<AppSettings> mutate)
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            settings.OpenCode ??= new OpenCodeSettings();
            mutate(settings);
            await _settingsService.SaveSettingsAsync(settings);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Failed to persist the OpenCode settings");
        }
    }

    // --- Public tab entry points (strip buttons toggle; row chips open directly) ---

    /// <summary>Opens the Overview tab (repo optional — the row chip passes its repo).</summary>
    public void OpenOverview(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.Overview;
        _ = LoadOverviewAsync();
    }

    /// <summary>Opens the Changes tab (repo optional — the row chip passes its repo).</summary>
    public void OpenChanges(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.Changes;
        _ = LoadChangedFilesAsync();
    }

    /// <summary>Opens the Pull Requests tab.</summary>
    public void OpenPullRequests(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.PullRequests;
        _ = LoadGitHubAsync();
    }

    /// <summary>Opens the Issues tab.</summary>
    public void OpenIssues(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.Issues;
        _ = LoadGitHubAsync();
    }

    /// <summary>Opens the Azure DevOps tab.</summary>
    public void OpenAzure(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.Azure;
        _ = LoadAzureAsync();
    }

    /// <summary>Opens the Git tab (branches + recent commits).</summary>
    public void OpenGit(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.Git;
        _ = LoadGitAsync();
    }

    /// <summary>
    /// Opens the OpenCode tab. A no-op while the integration is disabled (the per-row
    /// button is hidden then, but the strip tab remains clickable).
    /// </summary>
    public void OpenOpenCode(Repo? repo = null)
    {
        if (!IsOpenCodeEnabled) return;
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.OpenCode;
        _ = LoadOpenCodeModelsAsync();
    }

    /// <summary>
    /// Header tab buttons: the panel is the repo view, so tabs only SWITCH — clicking
    /// the active tab does nothing (there is no collapse-to-strip; leaving the repo
    /// view is "Back to Repositories", which hides the whole bar).
    /// </summary>
    [RelayCommand] private void ToggleOverviewTab() { if (ActiveTab != BottomBarTab.Overview) OpenOverview(); }
    [RelayCommand] private void ToggleChangesTab() { if (ActiveTab != BottomBarTab.Changes) OpenChanges(); }
    [RelayCommand] private void TogglePullRequestsTab() { if (ActiveTab != BottomBarTab.PullRequests) OpenPullRequests(); }
    [RelayCommand] private void ToggleIssuesTab() { if (ActiveTab != BottomBarTab.Issues) OpenIssues(); }
    [RelayCommand] private void ToggleAzureTab() { if (ActiveTab != BottomBarTab.Azure) OpenAzure(); }
    [RelayCommand] private void ToggleGitTab() { if (ActiveTab != BottomBarTab.Git) OpenGit(); }
    [RelayCommand] private void ToggleOpenCodeTab() { if (ActiveTab != BottomBarTab.OpenCode) OpenOpenCode(); }
}
