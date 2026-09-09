using System.Collections.ObjectModel;
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

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// Binding adapter for the Repos page's bottom panels — the bar's SHELL. Owns the
/// selected repo (the header, the tab badges and every panel target it), the tab
/// switching, the cross-panel aggregates (tab header totals, empty-state notes, the
/// Overview card pipeline line) and the OpenCode integration snapshot; the per-tab
/// state lives on the child view models it creates:
/// <see cref="Changes"/> (commit workspace), <see cref="GitHub"/> and
/// <see cref="Azure"/> (the provider panels behind the four activity tabs). The
/// Overview tab is a pure composite over the shell and those panels and binds the
/// shell directly.
/// <para>
/// The OpenCode launch UI lives in the tool drawer
/// (<see cref="Tools.Views.Components.OpenCodeSettingsComponent"/>); this VM keeps
/// only its integration flag/availability snapshot (which the row buttons, the wand
/// and the drawer's own seeding read) and the entry point that opens the drawer.
/// </para>
/// Unlike the transient drawer ViewModels this one is a singleton: it lives as long
/// as the window, so the panels' state survives page navigation.
/// <para>
/// The bar stays hidden until a repo is selected from the table — a row press or any
/// row chip routing to a tab (constructor-injected reference; every row chip that
/// used to open a modal dialog routes to the matching tab, passing its row's repo).
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
    private readonly IOpenCodeRunService _openCodeRunService;
    private readonly ICommitMessagePromptService _commitMessagePromptService;
    private readonly INotificationService _notificationService;
    private readonly IClipboardService _clipboardService;
    private readonly IToolDrawerService _toolDrawerService;

    private ReposSettings _reposSettings = new();
    private OpenCodeSettings _openCodeSettings = new();

    /// <summary>Services and settings the child panels read through the shell — the
    /// bar's dependency list stays injected once.</summary>
    internal IGitStatusService GitStatusService => _gitStatusService;
    internal IGitHubService GitHubService => _gitHubService;
    internal IAzureDevOpsService AzureDevOpsService => _azureDevOpsService;
    internal IProcessLauncher ProcessLauncher => _processLauncher;
    internal INotificationService Notifications => _notificationService;
    internal IClipboardService Clipboard => _clipboardService;
    internal IToolDrawerService Drawers => _toolDrawerService;
    internal ReposSettings ReposSettings => _reposSettings;

    /// <summary>The Changes tab (commit workspace): branch toolbar, staged/unstaged
    /// tree, staging, commit box with the wand, recent commits.</summary>
    public ChangesTabViewModel Changes { get; }

    /// <summary>The GitHub panel: pull requests + issues behind the Pull Requests and
    /// Issues tabs (and the Overview cards' GitHub previews).</summary>
    public GitHubPanelViewModel GitHub { get; }

    /// <summary>The Azure DevOps panel: pull requests, work items and pipeline runs
    /// behind the Azure tab (and the GitHub tabs' Azure sections).</summary>
    public AzurePanelViewModel Azure { get; }

    /// <summary>
    /// Guards the repo-dropdown rebuild posted from <see cref="IRepoService.Changed"/>:
    /// one scan raises Changed several times, and a single dispatcher pass per burst is enough.
    /// </summary>
    private bool _reposRebuildPosted;

    public BottomBarViewModel(
        ISettingsService settingsService,
        IRepoService repoService,
        IGitStatusService gitStatusService,
        IGitHubService gitHubService,
        IAzureDevOpsService azureDevOpsService,
        IProcessLauncher processLauncher,
        IOpenCodeRunService openCodeRunService,
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
        _openCodeRunService = openCodeRunService;
        _commitMessagePromptService = commitMessagePromptService;
        _notificationService = notificationService;
        _clipboardService = clipboardService;
        _toolDrawerService = toolDrawerService;

        var messageGenerator = new CommitMessageGenerator(openCodeRunService, commitMessagePromptService);
        Changes = new ChangesTabViewModel(this, messageGenerator);
        GitHub = new GitHubPanelViewModel(this);
        Azure = new AzurePanelViewModel(this);

        // The tab header totals and the empty-state notes read both providers' state —
        // whichever panel settles, the aggregates re-raise.
        GitHub.StateChanged += RaisePanelAggregates;
        Azure.StateChanged += RaisePanelAggregates;

        _repoService.Changed += OnRepoServiceChanged;
        _ = InitializeAsync();
    }

    /// <summary>Re-raises the shell's cross-panel bindings: the tab header totals, the
    /// shared tabs' empty-state notes and the Overview card's pipeline health line.</summary>
    private void RaisePanelAggregates()
    {
        OnPropertyChanged(nameof(OpenPullRequestCount));
        OnPropertyChanged(nameof(OpenIssueCount));
        OnPropertyChanged(nameof(ShowPullRequestsEmpty));
        OnPropertyChanged(nameof(ShowIssuesEmpty));
        RaisePipelineStatus();
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
            RefreshOpenCodeAvailability();

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
            bool isRealSwitch = !BottomBarPanelViewModel.IsSameRepo(value, _observedRepo);
            Changes.OnObservedRepoChanged(isRealSwitch);

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
            _ = Changes.LoadBranchesAsync();
            ReloadActiveTab();
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
            or nameof(Repo.GitToPushCount)
            or nameof(Repo.GitToPullCount)
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

    /// <summary>Raises every computed property derived from <see cref="SelectedRepo"/>
    /// — the shell's own badges and the Changes toolbar's mirrors.</summary>
    private void RaiseRepoDerived()
    {
        OnPropertyChanged(nameof(ChangesCount));
        OnPropertyChanged(nameof(ShowChangesBadge));
        OnPropertyChanged(nameof(PullRequestCount));
        OnPropertyChanged(nameof(ShowPullRequestBadge));
        OnPropertyChanged(nameof(IssueCount));
        OnPropertyChanged(nameof(ShowIssueBadge));
        OnPropertyChanged(nameof(SelectedRepoFolderPath));
        OnPropertyChanged(nameof(HasSelectedRepoGitHubUrl));
        OnPropertyChanged(nameof(SelectedRepoGitHubDisplayUrl));
        Changes.RaiseRepoMirrors();
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
    /// availability (the EnableOpenCode toggle) re-resolve immediately.
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
        IsBarVisible = true;
        if (!ReferenceEquals(repo, SelectedRepo))
        {
            SelectedRepo = repo;
        }
    }

    /// <summary>
    /// The load routine behind each tab — the one dispatch point shared by opening a
    /// tab (<see cref="OpenTab"/>) and reloading the open one
    /// (<see cref="ReloadActiveTab"/>). The shared GitHub tabs also kick the Azure load
    /// so their Azure sections fill even when entered without passing Overview;
    /// <see cref="BottomBarTab.None"/> has no panel to load.
    /// </summary>
    private Func<Task>? TabLoader(BottomBarTab tab) => tab switch
    {
        BottomBarTab.Overview => LoadOverviewAsync,
        BottomBarTab.Changes => () => Changes.LoadTabAsync(),
        BottomBarTab.PullRequests => LoadGitHubTabAsync,
        BottomBarTab.Issues => LoadGitHubTabAsync,
        BottomBarTab.Azure => () => Azure.LoadAzureAsync(),
        _ => null,
    };

    /// <summary>The shared GitHub tabs' load: the GitHub lists plus — when the Azure
    /// column is on — the Azure sections shown alongside them.</summary>
    private Task LoadGitHubTabAsync()
    {
        _ = GitHub.LoadAsync();
        LoadAzureForSharedTabs();
        return Task.CompletedTask;
    }

    /// <summary>Reloads whichever panel is open after the target repo changed underneath it.</summary>
    private void ReloadActiveTab()
    {
        if (TabLoader(ActiveTab) is { } load)
        {
            _ = load();
        }
    }

    /// <summary>True while the header's refresh is reloading the open panel; gates
    /// re-entrant refresh clicks (the tab loaders set their own flags underneath).</summary>
    [ObservableProperty]
    private bool _isPanelRefreshing;

    partial void OnIsPanelRefreshingChanged(bool value) => RefreshPanelCommand.NotifyCanExecuteChanged();

    private bool CanRefreshPanel() => !IsPanelRefreshing;

    /// <summary>Refreshes the open panel's data (the header's refresh button): awaits
    /// the active tab's loader under a gate so it can't be spammed into concurrent
    /// reloads. The GitHub tabs' loader fans out internally and returns early — their
    /// in-flight refreshes are guarded inside the panels' refresh twins.</summary>
    [RelayCommand(CanExecute = nameof(CanRefreshPanel))]
    private async Task RefreshPanelAsync()
    {
        if (TabLoader(ActiveTab) is not { } load) return;

        IsPanelRefreshing = true;
        try
        {
            await load();
        }
        finally
        {
            IsPanelRefreshing = false;
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
    public string PipelineStatusText => Azure.AzurePipelineRuns.Count == 0 ? "No pipeline runs"
        : Azure.AzurePipelineRuns.Any(r => r.IsFailed) ? "Checks failing"
        : Azure.AzurePipelineRuns.Any(r => r.IsRunning) ? "Pipelines running"
        : "All checks passing";

    /// <summary>Whether the Pipelines card shows a failing state (drives its color).</summary>
    public bool IsPipelineFailing => Azure.AzurePipelineRuns.Any(r => r.IsFailed);

    /// <summary>Whether the Pipelines card shows a running state.</summary>
    public bool IsPipelineRunning => !IsPipelineFailing && Azure.AzurePipelineRuns.Any(r => r.IsRunning);

    /// <summary>Whether the Pipelines card shows a passing state.</summary>
    public bool IsPipelinePassing => !IsPipelineFailing && !IsPipelineRunning && Azure.AzurePipelineRuns.Count > 0;

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
    /// the static repository details. The list loads reuse the panels' collections, so
    /// a switch from Overview to a tab shows instantly-populated data.
    /// </summary>
    private async Task LoadOverviewAsync()
    {
        _ = Changes.LoadChangedFilesAsync();
        if (IsGitHubEnabled)
        {
            _ = GitHub.LoadAsync();
        }
        if (IsAzureDevOpsEnabled)
        {
            _ = Azure.LoadAzureAsync();
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

        try
        {
            var details = await _gitHubService.GetRepoDetailsAsync(repo);
            if (!ReferenceEquals(SelectedRepo, repo)) return; // repo switched while loading
            SelectedRepoDetails = details;
        }
        catch (Exception ex)
        {
            // Fire-and-forget from the Overview load — without this catch the failure
            // vanishes into a discarded task and the details card just stays empty.
            Log.Logger.Error(ex, "Repo details load failed for {FolderPath}", repo.FolderPath);
        }
    }

    // --- Cross-panel aggregates (tab header totals + shared tabs' empty-state notes) ---

    /// <summary>Whether the GitHub tabs show at all (mirrors the GitHub column setting).</summary>
    [ObservableProperty]
    private bool _isGitHubEnabled;

    /// <summary>Whether the Azure tab shows at all (mirrors the Azure column setting).</summary>
    [ObservableProperty]
    private bool _isAzureDevOpsEnabled;

    /// <summary>Tab header totals: GitHub items plus the Azure DevOps ones shown in the
    /// same tabs' Azure sections.</summary>
    public int OpenPullRequestCount => GitHub.PullRequests.Count + Azure.AzurePullRequests.Count;

    public int OpenIssueCount => GitHub.Issues.Count + Azure.AzureWorkItems.Count;

    /// <summary>Empty only when the GitHub list is empty AND the Azure section has
    /// nothing to show either (settled: loaded, or the column being off) — otherwise
    /// the note would sit above live Azure rows.</summary>
    public bool ShowPullRequestsEmpty => GitHub.HasLoaded && !GitHub.IsUnavailable && GitHub.PullRequests.Count == 0
        && (!IsAzureDevOpsEnabled || (Azure.AzureHasLoaded && !Azure.HasAzurePullRequests));

    public bool ShowIssuesEmpty => GitHub.HasLoaded && !GitHub.IsUnavailable && GitHub.Issues.Count == 0
        && (!IsAzureDevOpsEnabled || (Azure.AzureHasLoaded && !Azure.HasAzureWorkItems));

    // --- GitHub repo-level link commands (the header kebab + the sidebars) ---

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

    /// <summary>
    /// Kicks the Azure load alongside the GitHub tabs so their Azure sections fill even
    /// when the tab was entered without passing Overview. No-op while the Azure column
    /// is off or a load is already running (the running one writes the same collections).
    /// </summary>
    private void LoadAzureForSharedTabs()
    {
        if (IsAzureDevOpsEnabled && !Azure.IsAzureRefreshing)
        {
            _ = Azure.LoadAzureAsync();
        }
    }

    // --- OpenCode (integration flag + settings snapshot; the launch UI lives in the
    //     OpenCodeSettings drawer component, which seeds from and persists through this VM) ---

    /// <summary>
    /// Whether the OpenCode integration is enabled (mirrors and persists
    /// <see cref="OpenCodeSettings.EnableOpenCode"/>). Toggling it is live: the per-row
    /// buttons re-evaluate via <see cref="OpenCodeStateChanged"/>.
    /// </summary>
    [ObservableProperty]
    private bool _isOpenCodeEnabled;

    /// <summary>
    /// Whether the OpenCode launch UI is usable: the integration enabled in settings
    /// (<see cref="OpenCodeSettings.EnableOpenCode"/>). Visibility is settings-driven —
    /// the configured opencode executable is resolved only at launch time.
    /// </summary>
    [ObservableProperty]
    private bool _hasOpenCode;

    partial void OnIsOpenCodeEnabledChanged(bool value)
    {
        _openCodeSettings.EnableOpenCode = value;
        RefreshOpenCodeAvailability();
        _ = PersistOpenCodeSettingAsync(o => o.EnableOpenCode = value);
        OpenCodeStateChanged?.Invoke();
    }

    private void RefreshOpenCodeAvailability()
    {
        HasOpenCode = IsOpenCodeEnabled;
    }

    partial void OnHasOpenCodeChanged(bool value)
    {
        // The wand's CanExecute reads this — the flip propagates without a repo reload.
        Changes.GenerateCommitMessageCommand.NotifyCanExecuteChanged();
        Changes.CommitCommand.NotifyCanExecuteChanged();
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
    /// Refreshes this VM's OpenCode snapshot from a settings object the Repo Settings
    /// drawer just persisted (the drawer VMs are transient and own no shared state): the
    /// wand and the per-row buttons read this snapshot, so it must track the file.
    /// Raises <see cref="OpenCodeStateChanged"/> so subscribers re-evaluate too.
    /// </summary>
    public void RefreshOpenCodeSnapshot(OpenCodeSettings fresh)
    {
        _openCodeSettings.DefaultModel = fresh.DefaultModel;
        _openCodeSettings.CommitModel = fresh.CommitModel;
        _openCodeSettings.EnableOpenCode = fresh.EnableOpenCode;
        OpenCodeStateChanged?.Invoke();
    }

    /// <summary>
    /// Persists one OpenCode settings mutation through the settings service's single-lock
    /// section-scoped <see cref="ISettingsService.UpdateAsync{TSection}"/> — deep copy in,
    /// one save out, the section resolved non-null before the mutation. Failures are
    /// logged: the in-memory toggle stays live either way.
    /// </summary>
    private async Task PersistOpenCodeSettingAsync(Action<OpenCodeSettings> mutate)
    {
        try
        {
            await _settingsService.UpdateAsync(mutate);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Failed to persist the OpenCode settings");
        }
    }

    // --- Public tab entry points (header tabs toggle; row chips open directly) ---

    /// <summary>
    /// The five open commands' core: point the bar at the chosen repo (the row chip's
    /// row; null keeps the current selection), switch the panel and kick its load —
    /// fire-and-forget, exactly as each copy did.
    /// </summary>
    private void OpenTab(BottomBarTab tab, Repo? repo)
    {
        SetTargetRepo(repo);
        ActiveTab = tab;
        if (TabLoader(tab) is { } load)
        {
            _ = load();
        }
    }

    /// <summary>Opens the Overview tab (repo optional — the row chip passes its repo).</summary>
    public void OpenOverview(Repo? repo = null) => OpenTab(BottomBarTab.Overview, repo);

    /// <summary>Opens the Changes tab (repo optional — the row chip passes its repo).</summary>
    public void OpenChanges(Repo? repo = null) => OpenTab(BottomBarTab.Changes, repo);

    /// <summary>Opens the Pull Requests tab (GitHub list plus the Azure DevOps section).</summary>
    public void OpenPullRequests(Repo? repo = null) => OpenTab(BottomBarTab.PullRequests, repo);

    /// <summary>Opens the Issues tab (GitHub list plus the Azure work items section).</summary>
    public void OpenIssues(Repo? repo = null) => OpenTab(BottomBarTab.Issues, repo);

    /// <summary>Opens the Azure DevOps tab.</summary>
    public void OpenAzure(Repo? repo = null) => OpenTab(BottomBarTab.Azure, repo);

    /// <summary>
    /// Opens the OpenCode settings drawer (the repo row's options icon, or the tools
    /// dropdown with no repo) on the given repo — model picker (which persists the
    /// default), instances, template, prompt and the launch button. A no-op while the
    /// integration is disabled. The drawer overlays the page, so the bar's own state is
    /// untouched; only the selected repo is updated when the call carries one.
    /// </summary>
    public void OpenOpenCode(Repo? repo = null)
    {
        if (!IsOpenCodeEnabled) return;
        repo ??= SelectedRepo;
        if (repo is not null && !ReferenceEquals(repo, SelectedRepo))
        {
            SelectedRepo = repo; // reloads the bar's data for the target; visibility untouched
        }
        _toolDrawerService.Open(ToolComponentMapper.OpenCodeKey, new OpenCodeSettingsContext(repo));
    }

    /// <summary>
    /// Header tab buttons: the panel is the repo view, so tabs only SWITCH — clicking
    /// the active tab does nothing (there is no collapse-to-strip; leaving the repo
    /// view is the header X, which hides the whole bar).
    /// </summary>
    private void ToggleTab(BottomBarTab tab)
    {
        if (ActiveTab != tab)
        {
            OpenTab(tab, repo: null);
        }
    }

    [RelayCommand]
    private void ToggleOverviewTab() => ToggleTab(BottomBarTab.Overview);

    [RelayCommand]
    private void ToggleChangesTab() => ToggleTab(BottomBarTab.Changes);

    [RelayCommand]
    private void TogglePullRequestsTab() => ToggleTab(BottomBarTab.PullRequests);

    [RelayCommand]
    private void ToggleIssuesTab() => ToggleTab(BottomBarTab.Issues);

    [RelayCommand]
    private void ToggleAzureTab() => ToggleTab(BottomBarTab.Azure);

    /// <summary>Cancels any in-flight wand run — app shutdown calls this so the
    /// opencode child process dies with the app.</summary>
    public void CancelCommitMessageGeneration() => Changes.CancelMessageGeneration();
}
