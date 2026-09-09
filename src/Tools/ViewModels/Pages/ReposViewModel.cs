using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Helpers;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Services.Abstractions;
using Tools.ViewModels.Components;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.ViewModels.Pages;

/// <summary>
/// Binding adapter for the Repos page. Delegates scanning, caching, and the shared
/// repo state to <see cref="IRepoService"/> (singleton), launching to
/// <see cref="ITerminalLauncher"/> (executable resolution, argument shapes and fallback
/// decisions), and tag persistence back through the service. The list projection
/// (filter/sort/tag state and its debounces) lives in <see cref="RepoListProjection"/>
/// and the header summary in <see cref="RepoHeaderTotals"/>; this VM forwards their
/// binding surface, owns the page lifecycle and keeps the view-specific state (column
/// and shortcut visibility). The OpenCode launch panel moved to the window's bottom
/// bar (<see cref="BottomBar.BottomBarViewModel"/>); this page keeps the per-row quick
/// launch and routes the row chips/affordances to the bar's tabs.
/// </summary>
public partial class ReposViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly IDialogService _dialogService;
    private readonly IRepoService _repoService;
    private readonly IGitStatusService _gitStatusService;
    private readonly IGitHubService _gitHubService;
    private readonly IAzureDevOpsService _azureDevOpsService;
    private readonly INugetLocalService _nugetLocalService;

    /// <summary>Both provider services behind the common contract, so the settings'
    /// Configure sweep is one loop (load and save).</summary>
    private readonly IEnumerable<IRepoActivityService> _activityServices;
    private readonly IProcessLauncher _processLauncher;
    private readonly ITerminalLauncher _terminalLauncher;
    private readonly INotificationService _notificationService;
    private readonly BottomBarViewModel _bottomBar;
    private ReposSettings _reposSettings = new();
    private OpenCodeSettings _openCodeSettings = new();

    private readonly RepoListProjection _list;
    private readonly RepoHeaderTotals _totals = new();

    [ObservableProperty]
    private bool _isRefreshing;

    partial void OnIsRefreshingChanged(bool value) => RefreshCommand.NotifyCanExecuteChanged();

    // --- OpenCode panel (transient state) ---

    /// <summary>
    /// Whether the OpenCode integration is enabled (mirrors <see cref="OpenCodeSettings.EnableOpenCode"/>).
    /// When false, the launch panel cannot open and all per-repo OpenCode UI is hidden.
    /// </summary>
    [ObservableProperty]
    private bool _isOpenCodeEnabled;

    // --- GitHub column visibility ---

    /// <summary>
    /// Whether the GitHub column shows (mirrors <see cref="ReposSettings.EnableGitHub"/>).
    /// When false the whole column cell collapses — and the GitHub service is configured
    /// off too, so no <c>gh</c> processes are spawned for a column that is not visible.
    /// </summary>
    [ObservableProperty]
    private bool _isGitHubColumnVisible;

    // --- Header totals (page-header summary; the running sums live in RepoHeaderTotals) ---

    /// <summary>Total open pull requests across all known repos.</summary>
    public int GitHubTotalPrCount => _totals.GitHubTotalPrCount;

    /// <summary>Total open issues across all known repos.</summary>
    public int GitHubTotalIssueCount => _totals.GitHubTotalIssueCount;

    /// <summary>Total uncommitted file changes across all known repos.</summary>
    public int GitTotalModifiedCount => _totals.GitTotalModifiedCount;

    /// <summary>
    /// Whether the header summary shows at all: the GitHub column must be enabled and at
    /// least one item open across the repos — an all-zero summary is noise, matching the
    /// per-row chips that hide when their count is zero.
    /// </summary>
    public bool HasGitHubTotals => IsGitHubColumnVisible
        && (GitHubTotalPrCount > 0 || GitHubTotalIssueCount > 0);

    /// <summary>
    /// Whether the changes stat shows: a red zero is pure noise, so unlike the GitHub
    /// pair (which shows together once either count is open) it waits for the first
    /// modified file.
    /// </summary>
    public bool HasChangesTotal => GitTotalModifiedCount > 0;

    /// <summary>
    /// Whether any header stat is visible — gates the hairline divider between the
    /// count badge and the stats, so the title cluster doesn't end in a dangling line.
    /// </summary>
    public bool HasHeaderStats => HasGitHubTotals || HasChangesTotal;

    partial void OnIsGitHubColumnVisibleChanged(bool value)
    {
        OnPropertyChanged(nameof(HasGitHubTotals));
        OnPropertyChanged(nameof(HasHeaderStats));
    }

    /// <summary>Recomputes the running header totals from the current repo set — the
    /// post-scan reseed that keeps the incremental folds drift-free.</summary>
    private void RefreshHeaderTotals() => _totals.Recalculate(_repoService.Repos);

    // --- Azure DevOps column visibility ---

    /// <summary>
    /// Whether the Azure DevOps column shows (mirrors <see cref="ReposSettings.EnableAzureDevOps"/>).
    /// When false the whole column cell collapses — and the Azure DevOps service is
    /// configured off too, so no REST calls are sent for a column that is not visible.
    /// </summary>
    [ObservableProperty]
    private bool _isAzureDevOpsColumnVisible;

    // --- Launch shortcut visibility (settings-driven) ---

    /// <summary>
    /// Whether the launch buttons show: purely the <c>Enable*</c> toggles in
    /// <see cref="ReposSettings"/> — the machine is not probed, so the flags are
    /// instant and deterministic. Recomputed whenever settings load or are saved (see
    /// <see cref="RefreshShortcutAvailability"/>); a button whose executable cannot be
    /// resolved reports that at launch time instead of hiding.
    /// </summary>
    [ObservableProperty]
    private bool _hasTerminal;

    /// <summary>Whether the open-solution button shows. It is a Visual Studio action and
    /// Visual Studio only exists on Windows, so it requires
    /// <see cref="ReposSettings.EnableVisualStudio"/> <em>and</em> Windows — no
    /// installation probe runs. A non-empty <see cref="ReposSettings.IdeExecutable"/>
    /// still overrides the .sln shell association when launching.</summary>
    [ObservableProperty]
    private bool _hasIde;

    /// <summary>Whether the VS Code button shows (<see cref="ReposSettings.EnableVSCode"/>).</summary>
    [ObservableProperty]
    private bool _hasVSCode;

    /// <summary>Whether the zcode button shows (<see cref="ReposSettings.EnableZCode"/>).</summary>
    [ObservableProperty]
    private bool _hasZCode;

    /// <summary>
    /// Whether the per-repo OpenCode buttons show: the integration must be enabled in
    /// settings (<see cref="OpenCodeSettings.EnableOpenCode"/>) — no installation probe.
    /// </summary>
    [ObservableProperty]
    private bool _hasOpenCode;

    /// <summary>
    /// Re-evaluates the launch-shortcut visibility flags from the current settings.
    /// Called after settings load and after the settings dialog saves. Visibility is
    /// settings-driven only (the Enable* toggles plus the Windows gate on Visual
    /// Studio); nothing probes the machine.
    /// </summary>
    private void RefreshShortcutAvailability()
    {
        HasTerminal = _reposSettings.EnableTerminal;

        // The open-solution button is a Visual Studio shortcut and Visual Studio only
        // exists on Windows: the toggle is OS-gated, and nothing detects an installation.
        HasIde = OperatingSystem.IsWindows() && _reposSettings.EnableVisualStudio;

        HasVSCode = _reposSettings.EnableVSCode;
        HasZCode = _reposSettings.EnableZCode;
        HasOpenCode = IsOpenCodeEnabled;
    }

    // --- List projection forwarding surface ---
    // The projection owns the filter/sort machinery; the page's XAML (and the window's
    // header search, which reads/writes FilterText in code-behind) keeps binding THIS
    // VM, so the surface forwards both values and change notifications.

    /// <summary>The sort orders offered in the toolbar selector, in dropdown order.</summary>
    public static IReadOnlyList<RepoSortOption> SortOptions => RepoListProjection.SortOptions;

    public string FilterText
    {
        get => _list.FilterText;
        set => _list.FilterText = value;
    }

    /// <summary>
    /// The currently selected entry of the toolbar sort selector. Seeded from the
    /// persisted <see cref="ReposSettings.SortMode"/> on page load; a user pick
    /// re-orders the list immediately and persists the mode back to settings.
    /// </summary>
    public RepoSortOption SelectedSortOption
    {
        get => _list.SelectedSortOption;
        set => _list.SelectedSortOption = value;
    }

    public ObservableCollection<Repo> FilteredRepos => _list.FilteredRepos;

    public ObservableCollection<TagFilter> TagFilters => _list.TagFilters;

    /// <summary>Whether the tracked-repo list is empty altogether — the empty-state
    /// overlay's "no repositories yet" variant (vs "the filters hid everything").</summary>
    public bool HasNoRepos => _list.HasNoRepos;

    /// <summary>Whether a search term or checked tag filter is currently applied —
    /// drives the empty-state overlay's "Clear filters" action.</summary>
    public bool IsFilterActive => _list.IsFilterActive;

    /// <summary>Whether the table has no rows at all (overlay visibility).</summary>
    public bool ShowReposEmptyNote => _list.ShowReposEmptyNote;

    public ICommand ClearTagFiltersCommand => _list.ClearTagFiltersCommand;

    public ICommand TagFilterChangedCommand => _list.TagFilterChangedCommand;

    public ReposViewModel(
        ISettingsService settingsService,
        IDialogService dialogService,
        IRepoService repoService,
        IGitStatusService gitStatusService,
        IGitHubService gitHubService,
        IAzureDevOpsService azureDevOpsService,
        INugetLocalService nugetLocalService,
        IEnumerable<IRepoActivityService> activityServices,
        IProcessLauncher processLauncher,
        ITerminalLauncher terminalLauncher,
        INotificationService notificationService,
        BottomBarViewModel bottomBar)
    {
        _settingsService = settingsService;
        _dialogService = dialogService;
        _repoService = repoService;
        _gitStatusService = gitStatusService;
        _gitHubService = gitHubService;
        _azureDevOpsService = azureDevOpsService;
        _nugetLocalService = nugetLocalService;
        _activityServices = activityServices;
        _processLauncher = processLauncher;
        _terminalLauncher = terminalLauncher;
        _notificationService = notificationService;
        _bottomBar = bottomBar;

        _list = new RepoListProjection(repoService);
        _list.Rebuilt += RefreshHeaderTotals;
        _list.SortModePicked += PersistSortMode;
        _list.RepoPropertyObserved += (repo, e) => _totals.HandleRepoProperty(repo, e.PropertyName);
        _list.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName is nameof(FilteredRepos)
                or nameof(TagFilters)
                or nameof(FilterText)
                or nameof(SelectedSortOption)
                or nameof(HasNoRepos)
                or nameof(IsFilterActive)
                or nameof(ShowReposEmptyNote))
            {
                OnPropertyChanged(e.PropertyName);
            }
        };
        _totals.PropertyChanged += (_, _) => RaiseTotalsDerived();

        _repoService.Changed += OnRepoChanged;
        _repoService.TagsChanged += OnRepoChanged;

        // The bottom bar's OpenCode toggle/default-model are live: re-evaluate the
        // per-row button availability when they change there. Detached on navigate-from
        // (the bar is a singleton; this VM is transient).
        bottomBar.OpenCodeStateChanged += OnBottomBarOpenCodeStateChanged;
    }

    /// <summary>Re-raises the totals and the flags derived from them — the tracker
    /// raises on its own surface, the page binds this one.</summary>
    private void RaiseTotalsDerived()
    {
        OnPropertyChanged(nameof(GitHubTotalPrCount));
        OnPropertyChanged(nameof(GitHubTotalIssueCount));
        OnPropertyChanged(nameof(GitTotalModifiedCount));
        OnPropertyChanged(nameof(HasGitHubTotals));
        OnPropertyChanged(nameof(HasChangesTotal));
        OnPropertyChanged(nameof(HasHeaderStats));
    }

    /// <summary>
    /// Mirrors the bottom bar's live OpenCode state (enabled flag / default model) into
    /// this page's snapshot so the per-row quick launch and availability flags stay
    /// fresh without re-reading settings.json.
    /// </summary>
    private void OnBottomBarOpenCodeStateChanged()
    {
        // Mirror into the observable property, not just the settings snapshot: the
        // refresh below and the launch guards read the property, and only it raises
        // change notifications for the row buttons.
        IsOpenCodeEnabled = _bottomBar.IsOpenCodeEnabled;
        _openCodeSettings.EnableOpenCode = IsOpenCodeEnabled;
        RefreshShortcutAvailability();
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override Task OnNavigatedFromAsync()
    {
        // Detach from the singletons so this Transient VM (rebuilt per navigation) is not
        // kept alive by them and does not receive further state changes.
        _repoService.Changed -= OnRepoChanged;
        _repoService.TagsChanged -= OnRepoChanged;
        _bottomBar.OpenCodeStateChanged -= OnBottomBarOpenCodeStateChanged;

        // Detach the projection's live-re-sort listeners, cancel its debounces and drop
        // the header totals' per-repo contributions: the repos are singleton-cached and
        // would otherwise keep this Transient VM alive across navigations.
        _list.Detach();
        _totals.ClearContributions();
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        _reposSettings = settings.Repos ?? new ReposSettings();
        _openCodeSettings = settings.OpenCode ?? new OpenCodeSettings();
        // Seed the sort selector from the persisted mode. Matching the exact option
        // instance keeps the no-change path silent (no re-apply, no save round-trip);
        // a non-default mode raises the change here and re-orders the initial list.
        SelectedSortOption = SortOptions.FirstOrDefault(o => o.Mode == _reposSettings.SortMode) ?? SortOptions[0];
        IsOpenCodeEnabled = _openCodeSettings.EnableOpenCode;
        IsGitHubColumnVisible = _reposSettings.EnableGitHub;
        IsAzureDevOpsColumnVisible = _reposSettings.EnableAzureDevOps;
        // Configure the activity services before loading repos: the full-refresh kicks
        // below and the services' own scan-triggered refreshes gate on these flags, so a
        // disabled column never queries (gh spawns / REST calls) even during the
        // initial scan burst.
        foreach (var activityService in _activityServices)
        {
            activityService.Configure(_reposSettings);
        }
        RefreshShortcutAvailability();
        await _repoService.EnsureLoadedAsync(_reposSettings);
        // The rebuild also re-seeds the header totals via Rebuilt: the repos are
        // singleton-cached and may still carry GitHub counts / git changes from an
        // earlier page visit (fresh loads start at zero, where the raise is a harmless
        // no-op for the UI).
        _list.Rebuild();
        RefreshHeaderTotals();

        // Kick the local git status checks in the background — the cards render instantly
        // with a "checking…" placeholder and the counts fill in as each repo's probe
        // completes. Only repos without a status yet need probing (first navigation, or
        // after a scan added repos); later navigations of the same session reuse the
        // statuses the earlier passes pushed onto the entities, instead of re-spawning
        // one git process per repo on every page visit.
        if (_repoService.Repos.Any(r => !r.GitStatusLoaded))
        {
            _ = _gitStatusService.RefreshAllAsync();
        }

        // Same lazy pattern for the GitHub column: only probe when the column is visible
        // and some repo has no GitHub data yet (first navigation or after new repos);
        // later navigations reuse the counts already pushed onto the entities.
        if (IsGitHubColumnVisible && _repoService.Repos.Any(r => !r.GitHubLoaded))
        {
            _ = _gitHubService.RefreshAllAsync();
        }

        // Same lazy pattern for the Azure DevOps column: only probe when the column is
        // visible, a token is configured and some repo has no Azure DevOps data yet
        // (first navigation or after new repos); later navigations reuse the counts
        // already pushed onto the entities.
        if (IsAzureDevOpsColumnVisible && _repoService.Repos.Any(r => !r.AzureDevOpsLoaded))
        {
            _ = _azureDevOpsService.RefreshAllAsync();
        }
    }

    private void OnRepoChanged(object? sender, EventArgs e)
    {
        // Wired to both Changed and TagsChanged: both mean "the projection may be stale"
        // (fresh scan data, or a tag/favorite edit that can reorder or re-filter).
        //
        // Note: the repo service's scan state is intentionally NOT mirrored onto the
        // base IsBusy here — only IsRefreshing gates the Refresh button, so a scan never
        // blocks the rest of the page. The cards/tags/list re-render from the service
        // snapshot below without disabling anything.
        //
        // A single scan raises Changed several times (start, after replacing the repos,
        // and in the finally block). Debounce so those collapse into one rebuild pass
        // rather than tearing the list down and rebuilding it per event. The rebuild's
        // Rebuilt raise re-seeds the header totals (a rescan can replace repo instances
        // — the totals the orphaned entities were carrying must drop).
        _list.ScheduleChangedRebuild();
    }

    /// <summary>
    /// Writes the picked sort mode into the persisted settings. The in-memory
    /// <see cref="ReposSettings"/> instance is shared with the settings service, so the
    /// next full settings save (e.g. from the settings dialog) keeps the choice too —
    /// the immediate save here just makes it survive an app crash/restart as well.
    /// </summary>
    private void PersistSortMode(RepoSortMode mode)
    {
        if (_reposSettings.SortMode == mode) return;
        _reposSettings.SortMode = mode;
        _ = PersistReposSettingsAsync();
    }

    private async Task PersistReposSettingsAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            settings.Repos = _reposSettings;
            await _settingsService.SaveSettingsAsync(settings);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Failed to persist the Repos page sort mode");
        }
    }

    // --- Launch commands ---
    // A double-click on a launch button would spawn two terminals/editors; a click on
    // the same target within a short window is treated as a repeat and dropped. The
    // window is per target (so launching repo A then repo B stays unaffected).

    /// <summary>Minimum spacing between launches of the same target.</summary>
    private static readonly TimeSpan LaunchRepeatWindow = TimeSpan.FromMilliseconds(750);

    private readonly Dictionary<string, long> _lastLaunchTicks = new();

    /// <summary>
    /// Records a launch of <paramref name="target"/> and returns whether it may proceed:
    /// false when the same target was launched within <see cref="LaunchRepeatWindow"/>.
    /// Commands run on the UI thread, so the dictionary needs no locking.
    /// </summary>
    private bool TryBeginLaunch(string target)
    {
        var now = Environment.TickCount64;
        if (_lastLaunchTicks.TryGetValue(target, out var last)
            && now - last < LaunchRepeatWindow.TotalMilliseconds)
        {
            return false;
        }

        _lastLaunchTicks[target] = now;
        return true;
    }

    [RelayCommand]
    private void OpenVisualStudio(Repo? repo)
    {
        if (repo?.SolutionPath is null || !TryBeginLaunch(repo.SolutionPath)) return;
        _terminalLauncher.OpenSolution(repo.SolutionPath, _reposSettings.IdeExecutable);
    }

    [RelayCommand]
    private void OpenFolder(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath) || !TryBeginLaunch(folderPath)) return;
        _terminalLauncher.OpenFolder(folderPath);
    }

    [RelayCommand]
    private async Task OpenWithVSCodeAsync(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath) || !TryBeginLaunch(folderPath)) return;
        await _terminalLauncher.OpenInVSCodeAsync(folderPath, _reposSettings.VSCodeExecutable, _reposSettings.VSCodeProfile);
    }

    [RelayCommand]
    private void OpenWithTerminal(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath) || !TryBeginLaunch(folderPath)) return;
        _terminalLauncher.OpenFolderInTerminal(folderPath, _reposSettings.TerminalExecutable);
    }

    // --- Row chips routing to the bottom bar ---

    /// <summary>
    /// Sentinel for <see cref="OpenTabFor"/>: the OpenCode settings drawer is not a bar
    /// tab (<see cref="BottomBarTab"/> has no member for it — opening it must not switch
    /// the bar's active tab), but its row chip shares the same route-through-the-bar
    /// shape as the tab chips.
    /// </summary>
    private const BottomBarTab OpenCodeDrawer = (BottomBarTab)(-1);

    /// <summary>
    /// Shared body of the row chips that hand their repo to the bottom bar: drop null
    /// rows (the designer-preview / recycled-container case), then open the bar's
    /// matching entry point on that repo. The bar replaces the old GitHub / Azure DevOps
    /// details modals and the per-row change list.
    /// </summary>
    private void OpenTabFor(BottomBarTab tab, Repo? repo)
    {
        if (repo is null) return;

        switch (tab)
        {
            case BottomBarTab.PullRequests: _bottomBar.OpenPullRequests(repo); break;
            case BottomBarTab.Issues: _bottomBar.OpenIssues(repo); break;
            case BottomBarTab.Changes: _bottomBar.OpenChanges(repo); break;
            case BottomBarTab.Azure: _bottomBar.OpenAzure(repo); break;
            case OpenCodeDrawer: _bottomBar.OpenOpenCode(repo); break;
        }
    }

    // --- GitHub column ---

    /// <summary>
    /// Opens the bottom bar's Pull Requests tab on the clicked row's repo: open pull
    /// requests listed as clickable links, seeded from the GitHub service cache and
    /// refreshed in the background.
    /// </summary>
    [RelayCommand]
    private void OpenPullRequests(Repo? repo) => OpenTabFor(BottomBarTab.PullRequests, repo);

    /// <summary>
    /// Opens the bottom bar's Issues tab on the clicked row's repo — the issues half of
    /// what the old GitHub details modal showed.
    /// </summary>
    [RelayCommand]
    private void OpenIssues(Repo? repo) => OpenTabFor(BottomBarTab.Issues, repo);

    /// <summary>
    /// Opens the bottom bar's Changes tab on the clicked row's repo (the branch pill's
    /// action): the working-tree change list with per-file status codes.
    /// </summary>
    [RelayCommand]
    private void OpenChanges(Repo? repo) => OpenTabFor(BottomBarTab.Changes, repo);

    // --- Azure DevOps column ---

    /// <summary>
    /// Opens the repo's Azure DevOps page in the browser. A no-op for repos without an
    /// Azure DevOps remote (their column cell shows nothing anyway).
    /// </summary>
    [RelayCommand]
    private void OpenAzureDevOpsRepo(Repo? repo)
    {
        if (string.IsNullOrWhiteSpace(repo?.AzureDevOpsRepoUrl)) return;
        _processLauncher.StartProcess(repo.AzureDevOpsRepoUrl);
    }

    /// <summary>
    /// Opens the bottom bar's Azure tab on the clicked row's repo: active pull requests,
    /// open work items and recent pipeline runs.
    /// </summary>
    [RelayCommand]
    private void OpenAzureDevOpsDetails(Repo? repo) => OpenTabFor(BottomBarTab.Azure, repo);

    /// <summary>
    /// Opens zcode on the repo folder. The AppImage-vs-terminal choice (and every
    /// resolution/argument decision) lives in <see cref="ITerminalLauncher.OpenZCode"/>.
    /// </summary>
    [RelayCommand]
    private void OpenWithZCode(Repo? repo)
    {
        if (repo?.FolderPath is null || !TryBeginLaunch(repo.FolderPath)) return;
        _terminalLauncher.OpenZCode(repo.FolderPath, _reposSettings.ZCodeExecutable, _reposSettings.TerminalExecutable);
    }

    // --- OpenCode ---

    /// <summary>
    /// Quick open: launches a single opencode instance in the repo folder with the
    /// default model from settings — no options, and no opencode catalog query (the
    /// button must not spawn the CLI to resolve a model). Without a configured default
    /// there is nothing to launch with: an error alert points at Repo Settings.
    /// </summary>
    [RelayCommand]
    private async Task QuickOpenOpenCodeAsync(Repo? repo)
    {
        if (repo?.FolderPath is null || !IsOpenCodeEnabled || !TryBeginLaunch(repo.FolderPath)) return;

        var settings = await _settingsService.GetSettingsAsync();
        var model = settings.OpenCode?.DefaultModel?.Trim();
        if (string.IsNullOrEmpty(model))
        {
            _notificationService.Show("No default OpenCode model — set it in Repo Settings", NotificationKind.Error);
            return;
        }

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null) return;

        var openCodeExe = ExecutableDefaults.ResolveCliForTerminal(_reposSettings.OpenCodeExecutable, "opencode");
        _terminalLauncher.LaunchOpenCode(terminalExe, openCodeExe, repo.FolderPath, model, string.Empty, 1);
    }

    /// <summary>
    /// Opens the OpenCode settings drawer on the clicked row's repo: the model picker
    /// (which persists the configured default model), commit model, instances, template,
    /// prompt and the launch button — a right-sidebar overlay that leaves the page and
    /// the bar untouched. Gated on the integration being enabled (mirrored live from the
    /// bar); the null-row drop happens in <see cref="OpenTabFor"/>.
    /// </summary>
    [RelayCommand]
    private void OpenOpenCode(Repo? repo)
    {
        if (!IsOpenCodeEnabled) return;
        OpenTabFor(OpenCodeDrawer, repo);
    }

    [RelayCommand]
    private Task ToggleFavoriteAsync(Repo? repo)
    {
        if (repo is null) return Task.CompletedTask;
        return _repoService.ToggleFavoriteAsync(repo);
    }

    // --- Settings & refresh ---

    /// <summary>
    /// Re-scans the configured folders and re-checks every repo's git status. Only the
    /// Refresh button is disabled for the duration (see <see cref="IsRefreshing"/>); the
    /// rest of the page remains fully interactive. Re-entrant-safe: a refresh already in
    /// progress ignores further clicks via <see cref="CanRefresh"/>.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanRefresh))]
    private async Task RefreshAsync()
    {
        IsRefreshing = true;
        try
        {
            // The service-level refresh outwaits its scan, and the status service's
            // coalescer lets this await join the loop the scan's completion raise
            // armed — so this window covers the whole cycle (scan + status pass) and
            // the data on screen is final when it ends.
            await _repoService.RefreshAsync(_reposSettings);
            await _gitStatusService.RefreshAllAsync();

            // Settle the projection NOW instead of leaving it to the Changed debounce:
            // its idle window would otherwise land the rebuilt list up to 100ms after
            // the busy state cleared, reading as data still trickling in. Cancel drops
            // the pending callback; the rebuild below is exactly what it would have run.
            _list.CancelPendingRebuild();
            _list.Rebuild(); // its Rebuilt raise re-seeds the header totals too
        }
        catch (Exception ex)
        {
            // An AsyncRelayCommand stashes a thrown exception in its unobserved
            // ExecutionTask — the busy state would clear with no feedback at all.
            Log.Logger.Error(ex, "Repos page refresh failed");
            _notificationService.Show("Refresh failed — see the logs", NotificationKind.Error);
        }
        finally
        {
            IsRefreshing = false;
        }
    }

    /// <summary>A refresh can start only when one isn't already running.</summary>
    private bool CanRefresh() => !IsRefreshing;

    // --- Add repository ---

    /// <summary>
    /// Opens the Add Repositories dialog: the user picks (or types) a folder, the dialog
    /// scans it for git repositories, and the checked findings come back as folder
    /// paths. Each path is appended to the persisted scan-folder roots (so the repos
    /// survive every future scan) and a rescan pulls them into the list. New repos show
    /// up with the scan's Changed raise; the git status pass fills their counts in the
    /// background.
    /// </summary>
    [RelayCommand]
    private async Task AddRepositoryAsync()
    {
        try
        {
            var added = await _dialogService.ShowAddRepositoryDialogAsync(_reposSettings, _repoService.Repos);
            if (added is null || added.Count == 0)
            {
                return;
            }

            var roots = new List<string>(_reposSettings.RepoScanFolders ?? Array.Empty<string>());
            var addedCount = 0;
            foreach (var path in added)
            {
                if (roots.Any(existing => RepoPath.SamePath(existing, path))) continue;
                roots.Add(path);
                addedCount++;
            }

            if (addedCount == 0)
            {
                _notificationService.Show("Repositories are already tracked", NotificationKind.Info);
                return;
            }

            _reposSettings.RepoScanFolders = roots.ToArray();
            await PersistReposSettingsAsync();
            await _repoService.RefreshAsync(_reposSettings);
            _notificationService.Show($"Added {addedCount} {(addedCount == 1 ? "repository" : "repositories")}", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error adding repositories");
            _notificationService.Show("Failed to add repositories", NotificationKind.Error);
        }
    }

    [RelayCommand]
    private async Task OpenSettingsAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();

            var edited = await _dialogService.ShowReposSettingsDialogAsync(
                settings.Repos ?? new ReposSettings(),
                settings.OpenCode ?? new OpenCodeSettings(),
                settings.NugetLocal?.EnableNuget ?? true);
            if (edited == null)
            {
                // User cancelled the dialog.
                return;
            }

            await SaveEditedSettingsAsync(settings, edited);
            ApplySavedSettings(edited);
            await RefreshAfterSettingsSaveAsync();
            _notificationService.Show("Settings saved", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error opening repo settings");
            _notificationService.Show("Failed to save settings", NotificationKind.Error);
        }
    }

    /// <summary>
    /// Lands both edited sections in ONE save — the dialog returned a composite so the
    /// pre-dialog snapshot can't clobber either section. The OpenCode edit surface is
    /// the two model fields only: they are merged into the existing section instead of
    /// replacing it, so flags the dialog doesn't show (e.g. EnableOpenCode) keep their
    /// stored values. Same merge discipline for the NuGet enable flag. The settings
    /// dialog doesn't touch the sort mode, but it may hand back a fresh instance — the
    /// live selection is carried over so the save doesn't revert it.
    /// </summary>
    private async Task SaveEditedSettingsAsync(AppSettings settings, ReposSettingsEditResult edited)
    {
        settings.Repos = edited.Repos;
        settings.OpenCode ??= new OpenCodeSettings();
        settings.OpenCode.DefaultModel = edited.OpenCode.DefaultModel;
        settings.OpenCode.CommitModel = edited.OpenCode.CommitModel;
        settings.NugetLocal ??= new NugetLocalSettings();
        settings.NugetLocal.EnableNuget = edited.EnableNuget;
        edited.Repos.SortMode = SelectedSortOption.Mode;
        await _settingsService.SaveSettingsAsync(settings);
    }

    /// <summary>Points the page (and every service keyed off the same settings) at the
    /// saved sections: column flags, activity services, launch shortcuts, the bottom
    /// bar's tab visibility and OpenCode snapshot.</summary>
    private void ApplySavedSettings(ReposSettingsEditResult edited)
    {
        _reposSettings = edited.Repos;
        IsGitHubColumnVisible = edited.Repos.EnableGitHub;
        IsAzureDevOpsColumnVisible = edited.Repos.EnableAzureDevOps;
        foreach (var activityService in _activityServices)
        {
            activityService.Configure(edited.Repos);
        }
        RefreshShortcutAvailability();
        // The bottom bar's tab visibility (GitHub/Azure) follows the same save, and
        // its OpenCode snapshot (default/commit model for the wand) is refreshed so
        // the next quick-launch/wand run sees the new models without a restart.
        _bottomBar.ApplySettings(edited.Repos);
        _bottomBar.RefreshOpenCodeSnapshot(edited.OpenCode);
    }

    /// <summary>The NuGet service re-reads the enable flag (stopping a running watch
    /// when disabled) and raises StateChanged, which flips the title-bar chip and the
    /// tools-menu entry live; the rescan pulls any newly added repositories in.</summary>
    private async Task RefreshAfterSettingsSaveAsync()
    {
        await _nugetLocalService.RefreshFromSettingsAsync();
        await _repoService.RefreshAsync(_reposSettings);
    }
}
