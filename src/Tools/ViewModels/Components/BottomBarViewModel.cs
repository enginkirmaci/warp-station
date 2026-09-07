using System.Collections.ObjectModel;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Input;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Helpers;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Formatters;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Windows;
using Tools.Services;
using Tools.Services.Abstractions;

namespace Tools.ViewModels.Components;

/// <summary>The panels the bottom bar can expand to. <see cref="None"/> has no
/// expanded panel (the bar shows nothing — the old strip is gone).</summary>
public enum BottomBarTab
{
    None = 0,
    Overview,
    Changes,
    PullRequests,
    Issues,
    Azure,
}

/// <summary>
/// Binding adapter for the Repos page's bottom panels. Owns the bar's selected repo (the
/// git controls, GitHub/Azure panels and the OpenCode launch all target it), the
/// expandable tab panels' data, and — relocated from the Repos page's overlay panel — the
/// whole OpenCode launch surface, which the <see cref="Tools.Views.Components.OpenCodePanel"/>
/// control shows as a full bottom panel of its own (the row options icon opens it; it
/// replaces the bar, which comes back on the next row press or row chip). Unlike the
/// transient page ViewModels this one is a singleton: it lives as long as the window, so
/// the panels' state survives page navigation.
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
    private readonly IOpenCodeRunService _openCodeRunService;
    private readonly IOpenCodeGridLauncher _openCodeGridLauncher;
    private readonly ICommitMessagePromptService _commitMessagePromptService;
    private readonly INotificationService _notificationService;
    private readonly IClipboardService _clipboardService;
    private readonly IToolDrawerService _toolDrawerService;

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
        IOpenCodeRunService openCodeRunService,
        IOpenCodeGridLauncher openCodeGridLauncher,
        ICommitMessagePromptService commitMessagePromptService,
        INotificationService notificationService,
        IClipboardService clipboardService,
        IToolDrawerService toolDrawerService)
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
        _openCodeRunService = openCodeRunService;
        _openCodeGridLauncher = openCodeGridLauncher;
        _commitMessagePromptService = commitMessagePromptService;
        _notificationService = notificationService;
        _clipboardService = clipboardService;
        _toolDrawerService = toolDrawerService;

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

            IsGitHubEnabled = _reposSettings.EnableGitHub;
            IsAzureDevOpsEnabled = _reposSettings.EnableAzureDevOps;
            IsOpenCodeEnabled = _openCodeSettings.EnableOpenCode;
            OpenCodeCommitModelText = _openCodeSettings.CommitModel ?? string.Empty;
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
            // A rescan re-resolves the same repo to a fresh instance — not a switch.
            // A real switch abandons the previous repo's staged set: an in-flight
            // message generation for it is now pointless, and its message must not
            // linger in the box where the next Commit would send it to the new repo.
            if (!IsSameRepo(value, _observedRepo))
            {
                CancelCommitMessageGeneration();
                CommitMessage = string.Empty;
            }

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
            PullCommand.NotifyCanExecuteChanged();
            PushCommand.NotifyCanExecuteChanged();
            LaunchOpenCodeCommand.NotifyCanExecuteChanged();
        }

        OnPropertyChanged(nameof(HasSelectedRepo));
        OnPropertyChanged(nameof(SelectedRepoName));
        RaiseRepoDerived();
    }

    /// <summary>The repo currently subscribed for badge/label forwarding. Never bound.</summary>
    private Repo? _observedRepo;

    /// <summary>
    /// Same repo across a rescan re-resolve (fresh instance, same folder) counts as
    /// unchanged; only a genuine switch (different folder, or null) returns false.
    /// </summary>
    private static bool IsSameRepo(Repo? a, Repo? b)
    {
        if (ReferenceEquals(a, b)) return true;
        if (a is null || b is null) return false;
        return RepoPath.SamePath(a.FolderPath ?? string.Empty, b.FolderPath ?? string.Empty);
    }

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
            or nameof(Repo.AzureDevOpsPrCount)
            or nameof(Repo.AzureDevOpsWorkItemCount)
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
        OnPropertyChanged(nameof(PullRequestCount));
        OnPropertyChanged(nameof(ShowPullRequestBadge));
        OnPropertyChanged(nameof(IssueCount));
        OnPropertyChanged(nameof(ShowIssueBadge));
        OnPropertyChanged(nameof(LastFetchText));
        OnPropertyChanged(nameof(HasFetched));
        OnPropertyChanged(nameof(SelectedRepoFolderPath));
        OnPropertyChanged(nameof(HasSelectedRepoGitHubUrl));
        OnPropertyChanged(nameof(SelectedRepoGitHubDisplayUrl));
    }

    /// <summary>Working-tree change count of the selected repo (the Overview card's footer).</summary>
    public int ChangesCount => SelectedRepo?.GitModifiedCount ?? 0;

    public bool ShowChangesBadge => ChangesCount > 0;

    /// <summary>Open pull requests of the selected repo (tab badge): GitHub plus Azure
    /// DevOps — the Azure side is 0 while the column is disabled, since the service
    /// never pushes counts then.</summary>
    public int PullRequestCount => (SelectedRepo?.GitHubPrCount ?? 0)
        + (SelectedRepo?.AzureDevOpsPrCount ?? 0);

    public bool ShowPullRequestBadge => PullRequestCount > 0;

    /// <summary>Open issues / work items of the selected repo (tab badge), same mix.</summary>
    public int IssueCount => (SelectedRepo?.GitHubIssueCount ?? 0)
        + (SelectedRepo?.AzureDevOpsWorkItemCount ?? 0);

    public bool ShowIssueBadge => IssueCount > 0;

    /// <summary>"Last fetched: 2m ago", or null when the repo was never fetched.</summary>
    public string? LastFetchText => SelectedRepo?.GitLastFetchLabel is { } label ? $"Last fetched: {label}" : null;

    public bool HasFetched => SelectedRepo?.GitLastFetchAt is not null;

    // --- Repo header (the page's title while a repo is selected) ---

    /// <summary>Folder path of the selected repo — the header's location link.</summary>
    public string? SelectedRepoFolderPath => SelectedRepo?.FolderPath;

    /// <summary>Whether the header can offer GitHub entry points for the selected repo.</summary>
    public bool HasSelectedRepoGitHubUrl => !string.IsNullOrWhiteSpace(SelectedRepo?.GitHubRepoUrl);

    /// <summary>
    /// The GitHub URL in display form — scheme stripped, so <c>github.com/owner/repo</c>
    /// (the Repository Details card's link label).
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

    /// <summary>Opens the selected repo's folder (the header's location link) in the file manager.</summary>
    [RelayCommand]
    private void OpenRepoFolder()
    {
        if (SelectedRepo?.FolderPath is { } path)
        {
            _processLauncher.StartProcess(path);
        }
    }

    /// <summary>
    /// The X button (far right of the bar header or the OpenCode panel header): closes
    /// whichever bottom panel is open — bar panel or OpenCode panel — and drops the
    /// selection, clearing the table row's highlight. A row press (Overview) or any
    /// row chip brings the bar back.
    /// </summary>
    [RelayCommand]
    private void Close()
    {
        ActiveTab = BottomBarTab.None;
        IsOpenCodePanelVisible = false;
        SelectedRepo = null;
        IsBarVisible = false;
    }

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
        IsGitHubEnabled = edited.EnableGitHub;
        IsAzureDevOpsEnabled = edited.EnableAzureDevOps;
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

    /// <summary>
    /// Whether the standalone OpenCode panel shows — the full bottom panel (same docked
    /// card as the bar's panel, no tab row) that the repo row's options icon opens.
    /// Mutually exclusive with the bar: opening either hides the other.
    /// </summary>
    [ObservableProperty]
    private bool _isOpenCodePanelVisible;

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

    /// <summary>Row press on the already-highlighted repo: closes the bar entirely —
    /// panel and selection — so the row loses its highlight, exactly like the
    /// header's close button; pressing the row again re-opens Overview. Any other row
    /// press opens as usual.</summary>
    public void ToggleForRepo(Repo repo)
    {
        if (IsBarVisible && SelectedRepo == repo && ActiveTab != BottomBarTab.None)
        {
            Close();
            return;
        }
        OpenForRepo(repo);
    }

    // --- Tabs ---

    [ObservableProperty]
    private BottomBarTab _activeTab;

    public bool IsPanelOpen => ActiveTab != BottomBarTab.None;

    // Per-tab panel heights: every tab shares the Overview-sized height except Changes,
    // which carries the full commit workspace (staged/unstaged lists, commit message box
    // and the recent-commits list) and starts taller. The panel's top-edge divider
    // (BottomBar's PanelResizer) drags these values around.
    private const double MinPanelHeight = 260d;
    private const double MaxPanelHeight = 780d;
    private double _overviewPanelHeight = 440d;
    private double _changesPanelHeight = 560d;

    /// <summary>The expanded panel's height — the active tab's own value (see above).</summary>
    public double PanelHeight => ActiveTab == BottomBarTab.Changes ? _changesPanelHeight : _overviewPanelHeight;

    /// <summary>Applies a drag delta (positive = taller) to the active tab's panel height.</summary>
    public void AdjustPanelHeight(double delta)
    {
        var current = ActiveTab == BottomBarTab.Changes ? _changesPanelHeight : _overviewPanelHeight;
        // Whole logical pixels only: sub-pixel heights re-rasterize without a visible
        // gain, and unchanged values must not trigger another layout pass (drag smoothness).
        var value = Math.Clamp(Math.Round(current + delta), MinPanelHeight, MaxPanelHeight);
        if (Math.Abs(value - current) < 0.5) return;
        if (ActiveTab == BottomBarTab.Changes) _changesPanelHeight = value;
        else _overviewPanelHeight = value;
        OnPropertyChanged(nameof(PanelHeight));
    }

    public bool IsActiveOverview => ActiveTab == BottomBarTab.Overview;
    public bool IsActiveChanges => ActiveTab == BottomBarTab.Changes;
    public bool IsActivePullRequests => ActiveTab == BottomBarTab.PullRequests;
    public bool IsActiveIssues => ActiveTab == BottomBarTab.Issues;
    public bool IsActiveAzure => ActiveTab == BottomBarTab.Azure;

    partial void OnActiveTabChanged(BottomBarTab value)
    {
        OnPropertyChanged(nameof(IsPanelOpen));
        OnPropertyChanged(nameof(PanelHeight));
        OnPropertyChanged(nameof(IsActiveOverview));
        OnPropertyChanged(nameof(IsActiveChanges));
        OnPropertyChanged(nameof(IsActivePullRequests));
        OnPropertyChanged(nameof(IsActiveIssues));
        OnPropertyChanged(nameof(IsActiveAzure));
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
        IsOpenCodePanelVisible = false;
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
                _ = LoadChangesTabAsync();
                break;
            case BottomBarTab.PullRequests:
            case BottomBarTab.Issues:
                _ = LoadGitHubAsync();
                LoadAzureForSharedTabs();
                break;
            case BottomBarTab.Azure:
                _ = LoadAzureAsync();
                break;
        }
    }

    /// <summary>Refreshes the open panel's data (the header's refresh button).</summary>
    [RelayCommand]
    private void RefreshPanel() => ReloadActiveTab();

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

    /// <summary>True while a pull is running; disables the Pull button.</summary>
    [ObservableProperty]
    private bool _isPulling;

    /// <summary>True while a push is running; disables the Push button.</summary>
    [ObservableProperty]
    private bool _isPushing;

    /// <summary>Pull and push exclude each other — concurrent syncs of one repo interleave badly.</summary>
    partial void OnIsPullingChanged(bool value)
    {
        PullCommand.NotifyCanExecuteChanged();
        PushCommand.NotifyCanExecuteChanged();
    }

    partial void OnIsPushingChanged(bool value)
    {
        PullCommand.NotifyCanExecuteChanged();
        PushCommand.NotifyCanExecuteChanged();
    }

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

    [RelayCommand(CanExecute = nameof(CanPull))]
    private async Task PullAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        IsPulling = true;
        try
        {
            var result = await _gitStatusService.PullAsync(repo);
            if (result.Success)
            {
                _notificationService.Show($"Pulled {repo.Name}", NotificationKind.Success);
                SyncBranchSelection(repo);
            }
            else
            {
                _notificationService.Show(
                    result.Error is { } error ? $"Pull failed for {repo.Name}: {error}" : $"Pull failed for {repo.Name}",
                    NotificationKind.Error);
            }
        }
        finally
        {
            IsPulling = false;
        }
    }

    private bool CanPull() => !IsPulling && !IsPushing && HasSelectedRepo;

    [RelayCommand(CanExecute = nameof(CanPush))]
    private async Task PushAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        IsPushing = true;
        try
        {
            var result = await _gitStatusService.PushAsync(repo);
            if (result.Success)
            {
                _notificationService.Show($"Pushed {repo.Name}", NotificationKind.Success);
            }
            else
            {
                _notificationService.Show(
                    result.Error is { } error ? $"Push failed for {repo.Name}: {error}" : $"Push failed for {repo.Name}",
                    NotificationKind.Error);
            }
        }
        finally
        {
            IsPushing = false;
        }
    }

    private bool CanPush() => !IsPulling && !IsPushing && HasSelectedRepo;

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

    /// <summary>
    /// Loads everything the Changes tab shows: the merged change list (keeps the
    /// Overview card's preview fresh), the staged/unstaged split and — when the
    /// Recent Commits section is expanded — the recent commits. Concurrent.
    /// </summary>
    private async Task LoadChangesTabAsync()
    {
        _ = LoadChangedFilesAsync();
        _ = LoadChangeGroupsAsync();
        if (ShowRecentCommits)
        {
            await LoadRecentCommitsAsync();
        }
    }

    // --- Changes tab: staged/unstaged split ---

    /// <summary>The index side of the selected repo's changes (the Staged section).</summary>
    [ObservableProperty]
    private ObservableCollection<GitChangedFile> _stagedFiles = new();

    /// <summary>The worktree side (the Unstaged section; untracked files included).</summary>
    [ObservableProperty]
    private ObservableCollection<GitChangedFile> _unstagedFiles = new();

    /// <summary>True while the staged/unstaged split is loading; gates the empty state.</summary>
    [ObservableProperty]
    private bool _isLoadingGroups;

    public int StagedCount => StagedFiles.Count;

    public int UnstagedCount => UnstagedFiles.Count;

    public bool ShowStagedSection => StagedFiles.Count > 0;

    public bool ShowUnstagedSection => UnstagedFiles.Count > 0;

    /// <summary>The whole tab's empty state: no staged and no unstaged entries at all.</summary>
    public bool ShowChangesTabEmpty => !IsLoadingFiles && !IsLoadingGroups
        && StagedFiles.Count == 0 && UnstagedFiles.Count == 0;

    partial void OnIsLoadingGroupsChanged(bool value) => RaiseChangeGroupsDerived();

    /// <summary>
    /// Raises every computed binding over <see cref="StagedFiles"/>/<see cref="UnstagedFiles"/>
    /// and re-evaluates the commit + wand commands, whose CanExecute read the staged count.
    /// The collections are mutated in place, so the count-derived bindings need this push.
    /// </summary>
    private void RaiseChangeGroupsDerived()
    {
        OnPropertyChanged(nameof(StagedCount));
        OnPropertyChanged(nameof(UnstagedCount));
        OnPropertyChanged(nameof(ShowStagedSection));
        OnPropertyChanged(nameof(ShowUnstagedSection));
        OnPropertyChanged(nameof(ShowChangesTabEmpty));
        CommitCommand.NotifyCanExecuteChanged();
        GenerateCommitMessageCommand.NotifyCanExecuteChanged();
    }

    private async Task LoadChangeGroupsAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            StagedFiles.Clear();
            UnstagedFiles.Clear();
            IsLoadingGroups = false;
            RaiseChangeGroupsDerived();
            return;
        }

        IsLoadingGroups = true;
        try
        {
            var groups = await _gitStatusService.GetChangeGroupsAsync(repo);
            if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading

            StagedFiles.Clear();
            foreach (var file in groups.Staged)
            {
                StagedFiles.Add(file);
            }
            UnstagedFiles.Clear();
            foreach (var file in groups.Unstaged)
            {
                UnstagedFiles.Add(file);
            }
        }
        finally
        {
            IsLoadingGroups = false;
            RaiseChangeGroupsDerived();
        }
    }

    /// <summary>Stages one file (+ button on an unstaged row).</summary>
    [RelayCommand]
    private async Task StageFileAsync(GitChangedFile? file)
    {
        var repo = SelectedRepo;
        if (repo is null || file is null) return;

        if (await _gitStatusService.StageAsync(repo, file.Path))
        {
            _ = LoadChangesTabAsync();
        }
        else
        {
            _notificationService.Show($"Could not stage {file.Path}", NotificationKind.Error);
        }
    }

    /// <summary>Unstages one file (− button on a staged row); the working tree keeps the change.</summary>
    [RelayCommand]
    private async Task UnstageFileAsync(GitChangedFile? file)
    {
        var repo = SelectedRepo;
        if (repo is null || file is null) return;

        if (await _gitStatusService.UnstageAsync(repo, file.Path))
        {
            _ = LoadChangesTabAsync();
        }
        else
        {
            _notificationService.Show($"Could not unstage {file.Path}", NotificationKind.Error);
        }
    }

    /// <summary>Stages everything, untracked files and deletions included (Stage All).</summary>
    [RelayCommand]
    private async Task StageAllAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        if (await _gitStatusService.StageAllAsync(repo))
        {
            _ = LoadChangesTabAsync();
        }
        else
        {
            _notificationService.Show($"Could not stage the changes of {repo.Name}", NotificationKind.Error);
        }
    }

    /// <summary>Unstages everything — index back to HEAD, working tree untouched (Unstage All).</summary>
    [RelayCommand]
    private async Task UnstageAllAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return;

        if (await _gitStatusService.UnstageAllAsync(repo))
        {
            _ = LoadChangesTabAsync();
        }
        else
        {
            _notificationService.Show($"Could not unstage the changes of {repo.Name}", NotificationKind.Error);
        }
    }

    // --- Changes tab: commit ---

    /// <summary>The commit message box's content. Optional: an empty box makes Commit
    /// generate the message first (the wand's run), then commit.</summary>
    [ObservableProperty]
    private string _commitMessage = string.Empty;

    /// <summary>The commit button's label: "Generate &amp; Commit" while the box is empty
    /// (the press writes the message first, exactly like the wand), "Commit" otherwise.</summary>
    public string CommitButtonText => string.IsNullOrWhiteSpace(CommitMessage)
        ? "Generate & Commit"
        : "Commit";

    /// <summary>True while the box is empty — the commit press generates the message first;
    /// picks the commit button's icon (wand vs check).</summary>
    public bool CommitWillAutoGenerate => string.IsNullOrWhiteSpace(CommitMessage);

    /// <summary>While the commit runs the button's static icon gives way to the spinning
    /// ring — these two gate the wand and the check.</summary>
    public bool CommitShowsWand => CommitWillAutoGenerate && !IsCommitting;
    public bool CommitShowsCheck => !CommitWillAutoGenerate && !IsCommitting;

    /// <summary>True while the commit runs (generate-then-commit); disables the Commit
    /// button.</summary>
    [ObservableProperty]
    private bool _isCommitting;

    private bool CanCommit() => HasSelectedRepo && !IsCommitting
        && StagedFiles.Count > 0
        && (!string.IsNullOrWhiteSpace(CommitMessage) || CanGenerateCommitMessage());

    partial void OnCommitMessageChanged(string value)
    {
        CommitCommand.NotifyCanExecuteChanged();
        OnPropertyChanged(nameof(CommitButtonText));
        OnPropertyChanged(nameof(CommitWillAutoGenerate));
        OnPropertyChanged(nameof(CommitShowsWand));
        OnPropertyChanged(nameof(CommitShowsCheck));
    }

    partial void OnIsCommittingChanged(bool value)
    {
        CommitCommand.NotifyCanExecuteChanged();
        GenerateCommitMessageCommand.NotifyCanExecuteChanged(); // no second wand run mid-commit
        OnPropertyChanged(nameof(CommitShowsWand));
        OnPropertyChanged(nameof(CommitShowsCheck));
    }

    /// <summary>Commits the staged index. With a typed message it commits that; with an
    /// empty box it first generates a message exactly like the wand (opencode over the
    /// staged diff) — the generated message lands in the box so it is visible and kept
    /// when the commit fails — then commits. Success clears the box and reloads the tab
    /// (the staged section empties, recent commits gain a row).</summary>
    [RelayCommand(CanExecute = nameof(CanCommit))]
    private async Task CommitAsync()
    {
        var repo = SelectedRepo;
        var message = CommitMessage.Trim();
        if (repo is null) return;

        IsCommitting = true;
        try
        {
            if (message.Length == 0)
            {
                var token = BeginCommitMessageGeneration();
                string? generated;
                try
                {
                    generated = await TryGenerateCommitMessageAsync(token);
                }
                catch (OperationCanceledException)
                {
                    // Repo switch or app shutdown mid-generation: abort quietly, the
                    // opencode child is already dead and repo's box must stay clean.
                    return;
                }
                finally
                {
                    EndCommitMessageGeneration();
                }

                if (!IsSameRepo(SelectedRepo, repo)) return; // repo switched while generating

                if (generated is null)
                {
                    _notificationService.Show("Could not generate a commit message", NotificationKind.Error);
                    return;
                }

                CommitMessage = generated;
                message = generated;
            }

            var hash = await _gitStatusService.CommitAsync(repo, message);
            if (hash is null)
            {
                _notificationService.Show("Commit failed — nothing staged, or git rejected it", NotificationKind.Error);
                return;
            }

            CommitMessage = string.Empty;
            _notificationService.Show(
                hash.Length > 0 ? $"Committed {hash} to {repo.Name}" : $"Committed to {repo.Name}",
                NotificationKind.Success);
            _ = LoadChangesTabAsync();
        }
        finally
        {
            IsCommitting = false;
        }
    }

    // --- Changes tab: recent commits ---

    /// <summary>Clicking a commit's hash copies the full SHA-1 to the clipboard;
    /// the row keeps showing the seven-char display form.</summary>
    [RelayCommand]
    private async Task CopyCommitHashAsync(GitCommitInfo? commit)
    {
        if (commit is null || string.IsNullOrEmpty(commit.Hash)) return;

        if (Application.Current?.ApplicationLifetime
            is IClassicDesktopStyleApplicationLifetime { MainWindow: { } window })
        {
            // Avalonia 12: plain text goes through the data-transfer API (SetTextAsync is gone)
            var transfer = new DataTransfer();
            transfer.Add(DataTransferItem.CreateText(commit.Hash));
            await window.Clipboard.SetDataAsync(transfer);
            _notificationService.Show($"Copied {commit.ShortHash} to clipboard", NotificationKind.Success);
        }
    }

    /// <summary>
    /// Opens the History drawer on a clicked commit: subject, actions (checkout /
    /// revert / copy SHA), the per-file change list and the web jump. The drawer
    /// receives this bar's selected repo together with the clicked row's commit.
    /// </summary>
    [RelayCommand]
    private void OpenCommitDetail(GitCommitInfo? commit)
    {
        var repo = SelectedRepo;
        if (commit is null || repo is null) return;
        _toolDrawerService.Open(ToolComponentMapper.CommitHistoryKey, new CommitHistoryContext(repo, commit));
    }

    // --- Changes tab: commit-message wand ---
    /// <summary>True while opencode writes the message; disables the wand button.</summary>
    [ObservableProperty]
    private bool _isGeneratingMessage;

    /// <summary>
    /// The in-flight message generation. The token rides into the run service, whose
    /// kill-on-cancel handler terminates the opencode process tree the moment it fires —
    /// on a repo switch (<see cref="OnSelectedRepoChanged"/>) or app shutdown, the CLI
    /// must not keep running in the background.
    /// </summary>
    private CancellationTokenSource? _commitMessageCts;

    /// <summary>Cancels any in-flight message generation. Safe to call anytime.</summary>
    public void CancelCommitMessageGeneration()
    {
        try { _commitMessageCts?.Cancel(); }
        catch (ObjectDisposedException) { /* the owning command already tore it down */ }
    }

    /// <summary>Supersedes any stray previous source and opens a new cancellation scope.</summary>
    private CancellationToken BeginCommitMessageGeneration()
    {
        CancelCommitMessageGeneration();
        _commitMessageCts?.Dispose();
        _commitMessageCts = new CancellationTokenSource();
        return _commitMessageCts.Token;
    }

    private void EndCommitMessageGeneration()
    {
        _commitMessageCts?.Dispose();
        _commitMessageCts = null;
    }

    /// <summary>Upper bound on the staged diff fed to the model, so the prompt stays sane.</summary>
    private const int MaxPromptPatchLength = 8000;

    private bool CanGenerateCommitMessage() => HasOpenCode && HasSelectedRepo
        && StagedFiles.Count > 0 && !IsGeneratingMessage && !IsCommitting;

    partial void OnIsGeneratingMessageChanged(bool value)
    {
        GenerateCommitMessageCommand.NotifyCanExecuteChanged();
        CommitCommand.NotifyCanExecuteChanged(); // CanCommit defers to the wand when the box is empty
    }

    partial void OnHasOpenCodeChanged(bool value)
    {
        GenerateCommitMessageCommand.NotifyCanExecuteChanged();
        CommitCommand.NotifyCanExecuteChanged(); // CanCommit defers to the wand when the box is empty
    }

    /// <summary>
    /// The wand button: asks opencode to write a commit message from the staged diff
    /// (plus the repo's recent subjects for tone) and drops it into the message box.
    /// Needs the OpenCode integration enabled and the CLI resolvable.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanGenerateCommitMessage))]
    private async Task GenerateCommitMessageAsync()
    {
        var repo = SelectedRepo;
        if (repo?.FolderPath is null) return;

        var token = BeginCommitMessageGeneration();
        IsGeneratingMessage = true;
        try
        {
            var message = await TryGenerateCommitMessageAsync(token);
            if (!IsSameRepo(SelectedRepo, repo)) return; // repo switched while generating
            if (message is null)
            {
                _notificationService.Show(
                    StagedFiles.Count == 0 ? "Nothing staged yet — stage changes first" : "Could not generate a commit message",
                    NotificationKind.Error);
                return;
            }

            CommitMessage = message;
        }
        catch (OperationCanceledException)
        {
            // Repo switch or app shutdown: the opencode child is already dead, the
            // box must stay untouched — drop the run without an error toast.
        }
        finally
        {
            EndCommitMessageGeneration();
            IsGeneratingMessage = false;
        }
    }

    /// <summary>
    /// The wand's core, shared with the auto-generate path of <see cref="CommitAsync"/>:
    /// runs the staged patch through opencode (the user-editable prompt template, the
    /// dedicated commit model or the default) and returns the cleaned message — null
    /// when nothing is staged, the CLI fails, or nothing usable came back. The caller
    /// reports the failure.
    /// </summary>
    private async Task<string?> TryGenerateCommitMessageAsync(CancellationToken cancellationToken)
    {
        var repo = SelectedRepo;
        if (repo?.FolderPath is null) return null;

        // Snapshot the prompt inputs before the first await: a repo switch mid-run
        // reloads StagedFiles/GitCommits for the new repo, and the prompt must not end
        // up mixing the old diff with the new repo's file list and tone context.
        var stagedPaths = StagedFiles.Select(f => f.Path).ToArray();
        var recentSubjects = GitCommits.Take(5).Select(c => c.Subject).ToArray();

        var patch = await _gitStatusService.GetStagedPatchAsync(repo);
        if (string.IsNullOrWhiteSpace(patch)) return null;

        var prompt = BuildCommitMessagePrompt(patch, stagedPaths, recentSubjects);
        var answer = await _openCodeRunService.RunAsync(
            _reposSettings.OpenCodeExecutable, ResolveWandModel(), prompt, cancellationToken);
        return CleanGeneratedMessage(answer);
    }

    /// <summary>
    /// Builds the wand's prompt: fills the user-editable template's placeholders with
    /// the staged file list, the (truncated) staged diff, and the repo's recent commit
    /// subjects as free-form context (tone reference). All three inputs are snapshots
    /// taken before the run started, never the live collections.
    /// </summary>
    private string BuildCommitMessagePrompt(string patch, string[] stagedPaths, string[] recentSubjects)
    {
        if (patch.Length > MaxPromptPatchLength)
        {
            patch = patch[..MaxPromptPatchLength] + "\n… (diff truncated)";
        }

        var fileList = string.Join(", ", stagedPaths);
        var context = recentSubjects.Length > 0
            ? "Recent commit subjects for tone:\n" + string.Join('\n', recentSubjects)
            : string.Empty;
        return _commitMessagePromptService.BuildPrompt(fileList, patch, context);
    }

    /// <summary>
    /// Normalizes the model's answer into a commit message: strips code fences and
    /// wrapping quotes, keeps the remaining lines (the template may emit subject +
    /// body + footer), and caps the total length. Returns null when nothing usable
    /// came back.
    /// </summary>
    private static string? CleanGeneratedMessage(string? answer)
    {
        if (string.IsNullOrWhiteSpace(answer)) return null;

        // Interior blank lines must survive (sections of a structured body are separated
        // by them), and so must a wrapped bullet's 2-space continuation indent — only
        // trailing whitespace, the fences and the outer padding are stripped.
        var lines = answer.Split('\n')
            .Select(l => l.TrimEnd())
            .SkipWhile(l => l.StartsWith("```", StringComparison.Ordinal) || l.Trim().Length == 0)
            .Reverse().SkipWhile(l => l.StartsWith("```", StringComparison.Ordinal) || l.Trim().Length == 0).Reverse()
            .ToList();
        var message = string.Join('\n', lines).Trim().Trim('"', '`').Trim();
        if (message.Length > 1500) message = message[..1500].TrimEnd();
        return message.Length == 0 ? null : message;
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

    // --- Recent commits (the Changes tab's bottom section) ---

    /// <summary>The selected repo's recent commits, newest first.</summary>
    [ObservableProperty]
    private ObservableCollection<GitCommitInfo> _gitCommits = new();

    /// <summary>True while the commit list is loading; gates the empty state.</summary>
    [ObservableProperty]
    private bool _isLoadingCommits;

    public bool ShowCommitsEmpty => !IsLoadingCommits && GitCommits.Count == 0;

    partial void OnIsLoadingCommitsChanged(bool value) => OnPropertyChanged(nameof(ShowCommitsEmpty));

    /// <summary>
    /// Whether the Recent Commits section shows under the Changes tab's workspace.
    /// Collapsed by default — the section header stays visible as the toggle and the
    /// history only loads once first expanded.
    /// </summary>
    [ObservableProperty]
    private bool _showRecentCommits;

    /// <summary>Header-row toggle: expanding with no commits loaded yet fetches them
    /// (the tab load skips the history while the section is collapsed).</summary>
    [RelayCommand]
    private async Task ToggleRecentCommitsAsync()
    {
        ShowRecentCommits = !ShowRecentCommits;
        if (ShowRecentCommits && GitCommits.Count == 0)
        {
            await LoadRecentCommitsAsync();
        }
    }

    /// <summary>Loads the recent commit list for the Changes tab's Recent Commits section.</summary>
    private async Task LoadRecentCommitsAsync()
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

    /// <summary>Tab header totals: GitHub items plus the Azure DevOps ones shown in the
    /// same tabs' Azure sections.</summary>
    public int OpenPullRequestCount => GitHubPullRequests.Count + AzurePullRequests.Count;

    public int OpenIssueCount => GitHubIssues.Count + AzureWorkItems.Count;

    partial void OnGitHubPullRequestsChanged(ObservableCollection<GitHubItem> value)
    {
        OnPropertyChanged(nameof(GitHubPullRequestsPreview));
        OnPropertyChanged(nameof(OpenPullRequestCount));
        OnPropertyChanged(nameof(ShowPullRequestsEmpty));
    }

    partial void OnGitHubIssuesChanged(ObservableCollection<GitHubItem> value)
    {
        OnPropertyChanged(nameof(GitHubIssuesPreview));
        OnPropertyChanged(nameof(OpenIssueCount));
        OnPropertyChanged(nameof(ShowIssuesEmpty));
    }

    [ObservableProperty]
    private bool _isGitHubRefreshing;

    [ObservableProperty]
    private bool _gitHubHasLoaded;

    [ObservableProperty]
    private bool _gitHubIsUnavailable;

    /// <summary>Empty only when the GitHub list is empty AND the Azure section has
    /// nothing to show either (settled: loaded, or the column being off) — otherwise
    /// the note would sit above live Azure rows.</summary>
    public bool ShowPullRequestsEmpty => GitHubHasLoaded && !GitHubIsUnavailable && GitHubPullRequests.Count == 0
        && (!IsAzureDevOpsEnabled || (AzureHasLoaded && !HasAzurePullRequests));

    public bool ShowIssuesEmpty => GitHubHasLoaded && !GitHubIsUnavailable && GitHubIssues.Count == 0
        && (!IsAzureDevOpsEnabled || (AzureHasLoaded && !HasAzureWorkItems));

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

    /// <summary>Opens the selected repo's GitHub pull-requests page (…/pulls).</summary>
    [RelayCommand]
    private void OpenGitHubPulls()
    {
        OpenGitHubSection("pulls");
    }

    /// <summary>Opens the selected repo's GitHub issues page (…/issues).</summary>
    [RelayCommand]
    private void OpenGitHubIssues()
    {
        OpenGitHubSection("issues");
    }

    /// <summary>Opens a section page (pulls, issues) of the selected repo's GitHub repo.</summary>
    private void OpenGitHubSection(string section)
    {
        if (string.IsNullOrWhiteSpace(SelectedRepo?.GitHubRepoUrl)) return;
        _processLauncher.StartProcess(SelectedRepo.GitHubRepoUrl.TrimEnd('/') + "/" + section);
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

    /// <summary>First three Azure items for the Overview cards' Azure sections.</summary>
    public IEnumerable<AzureDevOpsItem> AzurePullRequestsPreview => AzurePullRequests.Take(3);

    public IEnumerable<AzureDevOpsItem> AzureWorkItemsPreview => AzureWorkItems.Take(3);

    partial void OnAzurePullRequestsChanged(ObservableCollection<AzureDevOpsItem> value)
    {
        OnPropertyChanged(nameof(HasAzurePullRequests));
        OnPropertyChanged(nameof(AzurePullRequestsPreview));
        OnPropertyChanged(nameof(OpenPullRequestCount));
        OnPropertyChanged(nameof(ShowPullRequestsEmpty));
    }

    partial void OnAzureWorkItemsChanged(ObservableCollection<AzureDevOpsItem> value)
    {
        OnPropertyChanged(nameof(HasAzureWorkItems));
        OnPropertyChanged(nameof(AzureWorkItemsPreview));
        OnPropertyChanged(nameof(OpenIssueCount));
        OnPropertyChanged(nameof(ShowIssuesEmpty));
    }
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
            OnPropertyChanged(nameof(HasAzurePullRequests));
            OnPropertyChanged(nameof(HasAzureWorkItems));
            OnPropertyChanged(nameof(HasAzurePipelineRuns));
            OnPropertyChanged(nameof(AzurePullRequestsPreview));
            OnPropertyChanged(nameof(AzureWorkItemsPreview));
            OnPropertyChanged(nameof(OpenPullRequestCount));
            OnPropertyChanged(nameof(OpenIssueCount));
            OnPropertyChanged(nameof(ShowPullRequestsEmpty));
            OnPropertyChanged(nameof(ShowIssuesEmpty));
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

    /// <summary>
    /// Kicks the Azure load alongside the GitHub tabs so their Azure sections fill even
    /// when the tab was entered without passing Overview. No-op while the Azure column
    /// is off or a load is already running (the running one writes the same collections).
    /// </summary>
    private void LoadAzureForSharedTabs()
    {
        if (IsAzureDevOpsEnabled && !IsAzureRefreshing)
        {
            _ = LoadAzureAsync();
        }
    }

    [RelayCommand(CanExecute = nameof(CanRefreshAzure))]
    private async Task RefreshAzureAsync()
    {
        var repo = SelectedRepo;
        if (repo is null || IsAzureRefreshing) return;

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

    // --- OpenCode panel (state shown by the OpenCodePanel control; relocated from the
    //     Repos page overlay panel, then from the bar's tab row to a panel of its own) ---

    /// <summary>
    /// Whether the OpenCode integration is enabled (mirrors and persists
    /// <see cref="OpenCodeSettings.EnableOpenCode"/> — previously settings.json-only). Toggling
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
        _openCodeSettings.EnableOpenCode = value;
        RefreshOpenCodeAvailability();
        _ = PersistOpenCodeSettingAsync(s => s.OpenCode.EnableOpenCode = value);
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
    /// The commit-model box's text buffer (the OpenCode panel's plain TextBox). Persisted
    /// by <see cref="SaveCommitModelAsync"/> when the box loses focus — empty text means
    /// "no dedicated commit model", and the wand then uses the default model.
    /// </summary>
    [ObservableProperty]
    private string _openCodeCommitModelText = string.Empty;

    /// <summary>
    /// Persists the commit-model box: empty/whitespace clears the dedicated commit model
    /// (the wand falls back to the configured default), anything else keeps the trimmed
    /// id verbatim. No-op when the value didn't change.
    /// </summary>
    public async Task SaveCommitModelAsync()
    {
        var desired = string.IsNullOrWhiteSpace(OpenCodeCommitModelText)
            ? null
            : OpenCodeCommitModelText.Trim();
        if (string.Equals(_openCodeSettings.CommitModel, desired, StringComparison.Ordinal)) return;

        _openCodeSettings.CommitModel = desired;
        await PersistOpenCodeSettingAsync(s => s.OpenCode.CommitModel = desired);
    }

    /// <summary>
    /// The model the wand should run with: the dedicated commit model when set, else the
    /// configured default (which may itself be empty — opencode then picks its own).
    /// </summary>
    public string? ResolveWandModel()
    {
        var configured = _openCodeSettings.CommitModel?.Trim();
        return !string.IsNullOrEmpty(configured) ? configured : _openCodeSettings.DefaultModel;
    }

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
        // Guard spans the WHOLE load — including the await and the deferred
        // SelectionChanged the ComboBox raises after its ItemsSource swap (a synchronous
        // flag reset misses that, and the phantom sentinel pick persisted null over the
        // configured CommitModel on every app start).
        _syncingOpenCodePickers = true;
        try
        {
            var cached = _openCodeModelService.GetCachedModels(_openCodeSettings.DefaultModel);
            if (cached.Count > 0)
                ApplyOpenCodeModels(cached);

            var models = await _openCodeModelService.GetModelsAsync(_reposSettings.OpenCodeExecutable, _openCodeSettings.DefaultModel);
            ApplyOpenCodeModels(models);
        }
        finally
        {
            Dispatcher.UIThread.Post(() => _syncingOpenCodePickers = false, DispatcherPriority.Background);
        }
    }

    /// <summary>
    /// True while <see cref="LoadOpenCodeModelsAsync"/> refreshes the model list: the
    /// ItemsSource swap transiently re-selects entries and re-fires the default-model
    /// picker's commit handler, which must not persist those phantom picks.
    /// </summary>
    private bool _syncingOpenCodePickers;

    /// <summary>
    /// Pushes <paramref name="models"/> into <see cref="OpenCodeModels"/>, selects the
    /// configured default or first entry (or clears the selection when empty), and
    /// refreshes the filter projection and the computed has/empty flags. A model the user
    /// already picked survives the refresh when it is still present. Callers own the
    /// <see cref="_syncingOpenCodePickers"/> guard — this runs inside it.
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
        if (!IsOpenCodePanelVisible) return; // phantom pick while the panel is hidden
        if (_syncingOpenCodePickers) return; // phantom pick from an option-list rebuild
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
            CloseOpenCodePanelToBar();
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

        CloseOpenCodePanelToBar();
    }

    private bool CanLaunchOpenCode() => HasOpenCode && HasSelectedRepo;

    /// <summary>After a launch attempt (or a missing terminal): the OpenCode panel
    /// closes and the bar returns on the repo view (Overview) — the closest surviving
    /// equivalent of the old return to the strip, which no longer exists.</summary>
    private void CloseOpenCodePanelToBar()
    {
        IsOpenCodePanelVisible = false;
        IsBarVisible = true;
        if (ActiveTab == BottomBarTab.None)
        {
            ActiveTab = BottomBarTab.Overview;
            _ = LoadOverviewAsync();
        }
    }

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

    // --- Public tab entry points (header tabs toggle; row chips open directly) ---

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
        _ = LoadChangesTabAsync();
    }

    /// <summary>Opens the Pull Requests tab (GitHub list plus the Azure DevOps section).</summary>
    public void OpenPullRequests(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.PullRequests;
        _ = LoadGitHubAsync();
        LoadAzureForSharedTabs();
    }

    /// <summary>Opens the Issues tab (GitHub list plus the Azure work items section).</summary>
    public void OpenIssues(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.Issues;
        _ = LoadGitHubAsync();
        LoadAzureForSharedTabs();
    }

    /// <summary>Opens the Azure DevOps tab.</summary>
    public void OpenAzure(Repo? repo = null)
    {
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.Azure;
        _ = LoadAzureAsync();
    }

    /// <summary>
    /// Opens the OpenCode panel — the full bottom panel that the repo row's options icon
    /// opens: model picker (which persists the default), instances, template, prompt and
    /// the launch button. A no-op while the integration is disabled. The panel REPLACES
    /// the bar in the page's bottom slot (the bar hides); a row press or row chip brings
    /// the bar back (and hides the panel via <see cref="SetTargetRepo"/>).
    /// </summary>
    public void OpenOpenCode(Repo? repo = null)
    {
        if (!IsOpenCodeEnabled) return;
        SetTargetRepo(repo);
        ActiveTab = BottomBarTab.None;
        IsBarVisible = false;
        IsOpenCodePanelVisible = true;
        _ = LoadOpenCodeModelsAsync();
    }

    /// <summary>
    /// Header tab buttons: the panel is the repo view, so tabs only SWITCH — clicking
    /// the active tab does nothing (there is no collapse-to-strip; leaving the repo
    /// view is the header X, which hides the whole bar).
    /// </summary>
    [RelayCommand] private void ToggleOverviewTab() { if (ActiveTab != BottomBarTab.Overview) OpenOverview(); }
    [RelayCommand] private void ToggleChangesTab() { if (ActiveTab != BottomBarTab.Changes) OpenChanges(); }
    [RelayCommand] private void TogglePullRequestsTab() { if (ActiveTab != BottomBarTab.PullRequests) OpenPullRequests(); }
    [RelayCommand] private void ToggleIssuesTab() { if (ActiveTab != BottomBarTab.Issues) OpenIssues(); }
    [RelayCommand] private void ToggleAzureTab() { if (ActiveTab != BottomBarTab.Azure) OpenAzure(); }
}
