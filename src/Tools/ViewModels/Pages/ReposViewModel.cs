using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Formatters;
using Tools.Library.Mvvm;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.Services.Abstractions;
using Tools.ViewModels.Components;

namespace Tools.ViewModels.Pages;

/// <summary>
/// One entry of the Repos page sort selector: the sort mode plus its display label.
/// A labeled wrapper (rather than binding the raw enum) keeps the dropdown text in
/// one place and works with compiled bindings without a value converter.
/// </summary>
public sealed record RepoSortOption(RepoSortMode Mode, string Label);

/// <summary>
/// Binding adapter for the Repos page. Delegates scanning, caching, and the shared
/// repo state to <see cref="IRepoService"/> (singleton), process launching to
/// <see cref="IProcessLauncher"/>, and tag persistence back through the service.
/// Holds only view-specific state: the text + tag filters, the sort selection and the
/// filtered projection. The OpenCode launch panel moved to the window's bottom bar
/// (<see cref="Components.BottomBarViewModel"/>); this page keeps the per-row quick
/// launch and routes the row chips/affordances to the bar's tabs.
/// </summary>
public partial class ReposViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly IDevToolsClient _devToolsClient;
    private readonly IDialogService _dialogService;
    private readonly IRepoService _repoService;
    private readonly IGitStatusService _gitStatusService;
    private readonly IGitHubService _gitHubService;
    private readonly IAzureDevOpsService _azureDevOpsService;
    private readonly IProcessLauncher _processLauncher;
    private readonly IOpenCodeModelService _openCodeModelService;
    private readonly INotificationService _notificationService;
    private readonly BottomBarViewModel _bottomBar;
    private ReposSettings _reposSettings = new();
    private OpenCodeSettings _openCodeSettings = new();

    /// <summary>
    /// Debounce timers for the filter and the service-changed handler. A burst of typing or
    /// the several <c>Changed</c> raises a single scan produces each cancel the pending
    /// callback and restart the window, so only one in-place <see cref="ApplyFilter"/> runs
    /// per burst instead of tearing down the list per keystroke / per event.
    /// </summary>
    private CancellationTokenSource? _filterDebounce;
    private CancellationTokenSource? _changedDebounce;

    /// <summary>Idle window for the search-box filter before the list is re-synced.</summary>
    private const int FilterDebounceMs = 150;

    /// <summary>
    /// Idle window for coalescing the multiple <c>Changed</c> raises a single scan emits
    /// (start, data-ready, finally) into one rebuild.
    /// </summary>
    private const int ChangedDebounceMs = 100;

    /// <summary>
    /// The repos currently wired to <see cref="OnRepoPropertyChanged"/> for live re-sorting
    /// and the GitHub header totals. The service raises <c>Changed</c> only around scans,
    /// but the background git status / GitHub passes push their results straight onto the
    /// entities afterwards — without listening to the entities, a Last-activity/Changes
    /// sort would keep its pre-probe order and the header totals would lag until the next
    /// unrelated rebuild. Rebuilt after every scan because a rescan can replace the repo
    /// instances.
    /// </summary>
    private readonly HashSet<Repo> _sortObservedRepos = new();

    [ObservableProperty]
    private string _filterText = string.Empty;

    /// <summary>The sort orders offered in the toolbar selector, in dropdown order.</summary>
    public static IReadOnlyList<RepoSortOption> SortOptions { get; } = new[]
    {
        new RepoSortOption(RepoSortMode.Name, "Name"),
        new RepoSortOption(RepoSortMode.LastActivity, "Last activity"),
        new RepoSortOption(RepoSortMode.Changes, "Changes"),
    };

    /// <summary>
    /// The currently selected entry of the toolbar sort selector. Seeded from the
    /// persisted <see cref="ReposSettings.SortMode"/> on page load; a user pick
    /// re-orders the list immediately and persists the mode back to settings.
    /// </summary>
    [ObservableProperty]
    private RepoSortOption _selectedSortOption = SortOptions[0];

    [ObservableProperty]
    private ObservableCollection<Repo> _filteredRepos = new();

    /// <summary>
    /// The checkable tag list shown in the left filter panel. Rebuilt from
    /// <see cref="IRepoService.AllTags"/> whenever the service changes, preserving
    /// existing check states by tag name so checking a tag survives a rescan.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<TagFilter> _tagFilters = new();

    /// <summary>
    /// Tracks an in-flight refresh (repo scan + git status pass) so only the Refresh
    /// button reflects it — the rest of the page (search, tags, cards, OpenCode panel,
    /// and the per-card "checking…" git placeholders) stays interactive throughout.
    /// Kept separate from the base <see cref="ViewModelBase.IsBusy"/> (which mirrors the
    /// repo service's scan state) so nothing else on the page is gated by a refresh.
    /// </summary>
    [ObservableProperty]
    private bool _isRefreshing;

    partial void OnIsRefreshingChanged(bool value) => RefreshCommand.NotifyCanExecuteChanged();

    // --- OpenCode panel (transient state) ---

    /// <summary>
    /// Whether the OpenCode integration is enabled (mirrors <see cref="OpenCodeSettings.Enabled"/>).
    /// When false, the launch panel cannot open and all per-repo OpenCode UI is hidden.
    /// </summary>
    [ObservableProperty]
    private bool _isOpenCodeEnabled;

    // --- GitHub column visibility ---

    /// <summary>
    /// Whether the GitHub column shows (mirrors <see cref="ReposSettings.ShowGitHubColumn"/>).
    /// When false the whole column cell collapses — and the GitHub service is configured
    /// off too, so no <c>gh</c> processes are spawned for a column that is not visible.
    /// </summary>
    [ObservableProperty]
    private bool _isGitHubColumnVisible;

    // --- GitHub totals (page-header summary) ---

    /// <summary>
    /// Total open pull requests across all known repos — the at-a-glance summary beside
    /// the page title. Unloaded and non-GitHub repos contribute zero. Refreshed when a
    /// repo's GitHub counts change (see <see cref="OnRepoPropertyChanged"/>) and after a
    /// scan replaces the repo set (see <see cref="RefreshGitHubTotals"/>).
    /// </summary>
    public int GitHubTotalPrCount => _repoService.Repos.Sum(r => r.GitHubPrCount);

    /// <summary>Total open issues across all known repos. See <see cref="GitHubTotalPrCount"/>.</summary>
    public int GitHubTotalIssueCount => _repoService.Repos.Sum(r => r.GitHubIssueCount);

    /// <summary>
    /// Whether the header summary shows at all: the GitHub column must be enabled and at
    /// least one item open across the repos — an all-zero summary is noise, matching the
    /// per-row chips that hide when their count is zero.
    /// </summary>
    public bool HasGitHubTotals => IsGitHubColumnVisible
        && (GitHubTotalPrCount > 0 || GitHubTotalIssueCount > 0);

    partial void OnIsGitHubColumnVisibleChanged(bool value) => OnPropertyChanged(nameof(HasGitHubTotals));

    /// <summary>
    /// Re-raises the GitHub totals after the repo set may have been replaced wholesale
    /// (initial load, rescan): the fresh entities start at zero, so a previously non-zero
    /// summary must drop without any single entity carrying a change notification.
    /// </summary>
    private void RefreshGitHubTotals()
    {
        OnPropertyChanged(nameof(GitHubTotalPrCount));
        OnPropertyChanged(nameof(GitHubTotalIssueCount));
        OnPropertyChanged(nameof(HasGitHubTotals));
    }

    // --- Azure DevOps column visibility ---

    /// <summary>
    /// Whether the Azure DevOps column shows (mirrors <see cref="ReposSettings.ShowAzureDevOpsColumn"/>).
    /// When false the whole column cell collapses — and the Azure DevOps service is
    /// configured off too, so no REST calls are sent for a column that is not visible.
    /// </summary>
    [ObservableProperty]
    private bool _isAzureDevOpsColumnVisible;

    // --- Launch shortcut availability (per PC) ---

    /// <summary>
    /// The launch buttons are only shown when their executable actually resolves on this
    /// machine (see <see cref="ExecutableDefaults"/>): a button whose target is missing
    /// would spawn a terminal "command not found" or nothing at all, so it is hidden
    /// instead. Recomputed whenever settings load or are saved —
    /// <see cref="RefreshShortcutAvailability"/>. Windows keeps the configured name
    /// verbatim (CreateProcess resolves it), so there a configured executable is always
    /// considered available and the buttons keep the pre-availability behavior.
    /// </summary>
    [ObservableProperty]
    private bool _hasTerminal;

    /// <summary>Whether the open-solution action can run. It is a Visual Studio action, so
    /// it needs Visual Studio installed; a non-empty <see cref="ReposSettings.IdeExecutable"/>
    /// overrides the check (Locate-verified on Linux) so e.g. Rider can be wired up
    /// deliberately. Windows without VS and the .sln association re-pointed elsewhere
    /// hides the button until an IDE is configured.</summary>
    [ObservableProperty]
    private bool _hasIde;

    [ObservableProperty]
    private bool _hasVSCode;

    [ObservableProperty]
    private bool _hasZCode;

    /// <summary>
    /// Whether the per-repo OpenCode buttons show: the integration must be enabled in
    /// settings <em>and</em> the configured opencode CLI must resolve on this machine.
    /// </summary>
    [ObservableProperty]
    private bool _hasOpenCode;

    /// <summary>
    /// Re-evaluates the per-PC launch-shortcut availability flags from the current
    /// settings. Called after settings load and after the settings dialog saves. The
    /// underlying probes are memoized per process (see <see cref="ExecutableDefaults"/>),
    /// so the filesystem/vswhere work happens once per executable — repeat calls only
    /// re-run when a configured value actually changed.
    /// </summary>
    private void RefreshShortcutAvailability()
    {
        HasTerminal = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable) is not null;

        // The open-solution button is a Visual Studio shortcut: only VS itself (or an
        // explicitly configured stand-in IDE) makes it available — a detected editor like
        // VS Code must not light it up, it has its own button.
        var configuredIde = _reposSettings.IdeExecutable?.Trim();
        HasIde = !string.IsNullOrEmpty(configuredIde)
            ? ExecutableDefaults.Locate(configuredIde) is not null
            : ExecutableDefaults.HasVisualStudio();

        HasVSCode = ExecutableDefaults.Locate(_reposSettings.VSCodeExecutable) is not null;
        HasZCode = ExecutableDefaults.Locate(_reposSettings.ZCodeExecutable) is not null;
        HasOpenCode = IsOpenCodeEnabled && ExecutableDefaults.Locate(_reposSettings.OpenCodeExecutable) is not null;
    }

    public ReposViewModel(
        ISettingsService settingsService,
        IDevToolsClient devToolsClient,
        IDialogService dialogService,
        IRepoService repoService,
        IGitStatusService gitStatusService,
        IGitHubService gitHubService,
        IAzureDevOpsService azureDevOpsService,
        IProcessLauncher processLauncher,
        IOpenCodeModelService openCodeModelService,
        INotificationService notificationService,
        BottomBarViewModel bottomBar)
    {
        _settingsService = settingsService;
        _devToolsClient = devToolsClient;
        _dialogService = dialogService;
        _repoService = repoService;
        _gitStatusService = gitStatusService;
        _gitHubService = gitHubService;
        _azureDevOpsService = azureDevOpsService;
        _processLauncher = processLauncher;
        _openCodeModelService = openCodeModelService;
        _notificationService = notificationService;
        _bottomBar = bottomBar;

        _repoService.Changed += OnRepoChanged;
        _repoService.TagsChanged += OnRepoChanged;

        // The bottom bar's OpenCode toggle/default-model are live: re-evaluate the
        // per-row button availability when they change there. Detached on navigate-from
        // (the bar is a singleton; this VM is transient).
        bottomBar.OpenCodeStateChanged += OnBottomBarOpenCodeStateChanged;
    }

    /// <summary>
    /// Mirrors the bottom bar's live OpenCode state (enabled flag / default model) into
    /// this page's snapshot so the per-row quick launch and availability flags stay
    /// fresh without re-reading settings.json.
    /// </summary>
    private void OnBottomBarOpenCodeStateChanged()
    {
        _openCodeSettings.Enabled = _bottomBar.IsOpenCodeEnabled;
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

        // Detach the live-re-sort listeners: the repos are singleton-cached and would
        // otherwise keep this Transient VM alive across navigations.
        DetachSortListeners();

        // Cancel any deferred filter/changed callbacks so a pending debounce does not fire
        // its UI-thread update after this VM is no longer the active page.
        _filterDebounce?.Cancel();
        _filterDebounce?.Dispose();
        _changedDebounce?.Cancel();
        _changedDebounce?.Dispose();
        _filterDebounce = null;
        _changedDebounce = null;
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
        IsOpenCodeEnabled = _openCodeSettings.Enabled;
        IsGitHubColumnVisible = _reposSettings.ShowGitHubColumn;
        IsAzureDevOpsColumnVisible = _reposSettings.ShowAzureDevOpsColumn;
        // Configure the GitHub service before loading repos: both the explicit kick below
        // and the service's own scan-triggered refresh gate on this flag, so a disabled
        // column never spawns gh even during the initial scan burst.
        _gitHubService.Configure(_reposSettings);
        // Same for the Azure DevOps service: the flag plus the configured token gate
        // every REST call, so a disabled column sends no requests at all.
        _azureDevOpsService.Configure(_reposSettings);
        RefreshShortcutAvailability();
        await _repoService.EnsureLoadedAsync(_reposSettings);
        RebuildTagFilters();
        RefreshSortListeners();
        // The repos are singleton-cached and may still carry GitHub counts from an earlier
        // page visit — seed the header totals from them (fresh loads start at zero, where
        // this raise is a harmless no-op for the UI).
        RefreshGitHubTotals();
        ApplyFilter();

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
        // rather than tearing the list down and rebuilding it per event.
        ScheduleChangedDebounce();
    }

    /// <summary>
    /// Coalesces a burst of <see cref="IRepoService.Changed"/> raises into a single
    /// tag-filter rebuild + in-place list sync on the UI thread.
    /// </summary>
    private void ScheduleChangedDebounce()
    {
        _changedDebounce?.Cancel();
        _changedDebounce?.Dispose();
        _changedDebounce = new CancellationTokenSource();
        var token = _changedDebounce.Token;
        Task.Run(async () =>
        {
            try
            {
                await Task.Delay(ChangedDebounceMs, token);
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    if (token.IsCancellationRequested) return;
                    RebuildTagFilters();
                    // A rescan can replace repo instances — re-wire the live-re-sort
                    // listeners to the fresh set before re-ordering, and drop the GitHub
                    // totals the orphaned entities were carrying.
                    RefreshSortListeners();
                    RefreshGitHubTotals();
                    ApplyFilter();
                });
            }
            catch (OperationCanceledException) { }
        });
    }

    partial void OnFilterTextChanged(string value) => ScheduleFilterDebounce();

    /// <summary>
    /// Coalesces a burst of keystrokes into a single in-place list sync so the cards are
    /// not torn down and rebuilt per character.
    /// </summary>
    private void ScheduleFilterDebounce()
    {
        _filterDebounce?.Cancel();
        _filterDebounce?.Dispose();
        _filterDebounce = new CancellationTokenSource();
        var token = _filterDebounce.Token;
        Task.Run(async () =>
        {
            try
            {
                await Task.Delay(FilterDebounceMs, token);
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    if (token.IsCancellationRequested) return;
                    ApplyFilter();
                });
            }
            catch (OperationCanceledException) { }
        });
    }

    /// <summary>
    /// Clears the search box and every tag checkbox (does not touch the tags on the
    /// repos themselves).
    /// </summary>
    [RelayCommand]
    private void ClearTagFilters()
    {
        FilterText = string.Empty;
        foreach (var tag in TagFilters)
            tag.IsChecked = false;
        ApplyFilter();
    }

    /// <summary>
    /// Called from the view when a tag checkbox is toggled, since TagFilter.IsChecked
    /// changes do not flow through this VM's own property-change pipeline.
    /// </summary>
    [RelayCommand]
    private void TagFilterChanged() => ApplyFilter();

    private void RebuildTagFilters()
    {
        var previous = TagFilters.ToDictionary(t => t.Name, t => t.IsChecked);
        var tags = _repoService.AllTags
            .OrderBy(t => t, StringComparer.OrdinalIgnoreCase)
            .ToList();

        var rebuilt = new ObservableCollection<TagFilter>();
        foreach (var name in tags)
        {
            var wasChecked = previous.TryGetValue(name, out var c) && c;
            rebuilt.Add(new TagFilter(name) { IsChecked = wasChecked });
        }

        TagFilters = rebuilt;
    }

    partial void OnSelectedSortOptionChanged(RepoSortOption? value)
    {
        if (value is null) return;
        ApplyFilter();
        PersistSortMode(value.Mode);
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

    /// <summary>
    /// Re-wires the live-re-sort listeners to the repos the service currently knows.
    /// Idempotent per pass; called on page load and after every scan so replaced repo
    /// instances don't leave the set holding (and keeping alive) stale ones.
    /// </summary>
    private void RefreshSortListeners()
    {
        foreach (var repo in _sortObservedRepos)
            repo.PropertyChanged -= OnRepoPropertyChanged;
        _sortObservedRepos.Clear();

        foreach (var repo in _repoService.Repos)
            _sortObservedRepos.Add(repo);

        foreach (var repo in _sortObservedRepos)
            repo.PropertyChanged += OnRepoPropertyChanged;
    }

    private void DetachSortListeners()
    {
        foreach (var repo in _sortObservedRepos)
            repo.PropertyChanged -= OnRepoPropertyChanged;
        _sortObservedRepos.Clear();
    }

    private void OnRepoPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        // Only the properties the sort keys read can change the ordering; ignoring the
        // rest (branch name, Azure counts, …) keeps a full probe pass from re-sorting for
        // nothing. Reuses the filter debounce so a burst of probe completions collapses
        // into one re-order.
        if (e.PropertyName is nameof(Repo.GitLastCommitAt)
            or nameof(Repo.GitModifiedCount)
            or nameof(Repo.GitToPushCount)
            or nameof(Repo.GitToPullCount))
        {
            ScheduleFilterDebounce();
        }

        // The GitHub probes push their counts from background gh continuations; the header
        // totals re-read the whole repo set per change, so raise per count kind. (Avalonia
        // marshals the binding updates onto the UI thread, same as the per-row chips.)
        if (e.PropertyName is nameof(Repo.GitHubPrCount))
        {
            OnPropertyChanged(nameof(GitHubTotalPrCount));
            OnPropertyChanged(nameof(HasGitHubTotals));
        }
        else if (e.PropertyName is nameof(Repo.GitHubIssueCount))
        {
            OnPropertyChanged(nameof(GitHubTotalIssueCount));
            OnPropertyChanged(nameof(HasGitHubTotals));
        }
    }

    /// <summary>
    /// Orders the filtered repos: favorites always float to the top (the star is a pin,
    /// in every mode), then the selected sort mode orders the rest, with the name as the
    /// stable tiebreaker. <see cref="Repo.GitLastCommitAt"/> nulls (not yet probed or no
    /// commits) sort last because DateTimeOffset? ascending puts null smallest and the
    /// ordering is descending.
    /// </summary>
    private IOrderedEnumerable<Repo> SortRepos(IEnumerable<Repo> repos)
    {
        var favoritesFirst = repos.OrderByDescending(r => r.IsFavorite);
        return SelectedSortOption.Mode switch
        {
            RepoSortMode.LastActivity => favoritesFirst
                .ThenByDescending(r => r.GitLastCommitAt)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            RepoSortMode.Changes => favoritesFirst
                .ThenByDescending(r => r.GitModifiedCount + r.GitToPushCount + r.GitToPullCount)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            _ => favoritesFirst.ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
        };
    }

    private void ApplyFilter()
    {
        var filter = FilterText?.Trim();
        var repos = _repoService.Repos;
        var checkedTags = TagFilters
            .Where(t => t.IsChecked)
            .Select(t => t.Name)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);

        IEnumerable<Repo> result = repos;
        if (checkedTags.Count > 0)
        {
            // OR: a repo passes if it has ANY of the checked tags.
            result = result.Where(r => r.Tags.Any(t => checkedTags.Contains(t.Name)));
        }

        if (!string.IsNullOrWhiteSpace(filter))
        {
            result = result.Where(r =>
                r.Name?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true
                || r.FolderPath?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true
                || r.SolutionPath?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true);
        }

        // Favorites always float to the top, then the selected sort mode (see SortRepos).
        var ordered = SortRepos(result).ToList();

        // Skip the sync when the projection is unchanged (e.g. adding a tag while no tag
        // filter is checked, or a search term that matches the same set): Clear/Add would
        // churn every recycled container and re-render the list for nothing. Same-count
        // lists are compared by reference — repos are shared instances, and Repo has no
        // value-equality that would catch a name/path edit anyway.
        if (ordered.Count == FilteredRepos.Count)
        {
            var unchanged = true;
            for (var i = 0; i < ordered.Count; i++)
            {
                if (ReferenceEquals(ordered[i], FilteredRepos[i])) continue;
                unchanged = false;
                break;
            }

            if (unchanged) return;
        }

        // Sync the existing collection in place rather than replacing it. Reassigning a new
        // instance here would force every card container to be torn down and rebuilt (and
        // with a non-virtualizing panel, re-realized up front). Clear/Add flow through
        // CollectionChanged so the virtualized ListBox only recycles affected containers, and
        // the count binding ({Binding FilteredRepos.Count}) updates from those same notifications.
        FilteredRepos.Clear();
        foreach (var repo in ordered)
            FilteredRepos.Add(repo);
    }

    // --- Launch commands ---

    [RelayCommand]
    private void OpenVisualStudio(Repo? repo)
    {
        if (repo?.SolutionPath is null) return;

        // Windows keeps the .sln shell association unless an IDE is configured; other
        // platforms open the solution in an auto-detected .NET IDE (e.g. Rider).
        var ide = ExecutableDefaults.ResolveIde(_reposSettings.IdeExecutable);
        if (ide is null)
        {
            if (OperatingSystem.IsWindows())
            {
                _processLauncher.StartProcess(repo.SolutionPath);
            }
            else
            {
                Log.Logger.Warning("OpenVisualStudio: no .NET IDE found; configure one in Repos settings");
            }
            return;
        }

        _processLauncher.StartProcess(ide, $"\"{repo.SolutionPath}\"", stripElectronEnvironment: true);
    }

    [RelayCommand]
    private void OpenFolder(string? folderPath) => _processLauncher.StartProcess(folderPath);

    [RelayCommand]
    private async Task OpenWithVSCodeAsync(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        // Resolve early: on Linux the GUI PATH can miss user-level installs, so the
        // fallback launch below needs the absolute path, not the bare name.
        var exe = ExecutableDefaults.Locate(_reposSettings.VSCodeExecutable)
                  ?? _reposSettings.VSCodeExecutable
                  ?? "code";

        // When a profile is configured, launch VS Code with it (--profile <name>);
        // otherwise open with the default profile (no extra arguments).
        var args = string.IsNullOrWhiteSpace(_reposSettings.VSCodeProfile)
            ? folderPath
            : $"--profile \"{_reposSettings.VSCodeProfile}\" \"{folderPath}\"";

        // Route through the DevTools service (named pipe). The service runs
        // non-elevated, so VS Code launches non-elevated even when Tools runs as admin.
        try
        {
            await _devToolsClient.SendProcessLaunchRequestAsync(exe, args);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenWithVSCode: pipe launch failed, falling back to direct launch");
            _processLauncher.StartProcess(exe, args, hidden: true, stripElectronEnvironment: true);
        }
    }

    [RelayCommand]
    private void OpenWithTerminal(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (exe is null) return;

        var args = TerminalArgumentFormatter.BuildArguments(exe, folderPath);
        _processLauncher.StartProcess(exe, args, stripElectronEnvironment: true);
    }

    // --- GitHub column ---

    /// <summary>
    /// Opens the repo's GitHub page in the browser. A no-op for repos without a GitHub
    /// remote (their column cell shows nothing anyway).
    /// </summary>
    [RelayCommand]
    private void OpenGitHubRepo(Repo? repo)
    {
        if (string.IsNullOrWhiteSpace(repo?.GitHubRepoUrl)) return;
        _processLauncher.StartProcess(repo.GitHubRepoUrl);
    }

    /// <summary>
    /// Opens the bottom bar's Pull Requests tab on the clicked row's repo: open pull
    /// requests listed as clickable links, seeded from the GitHub service cache and
    /// refreshed in the background. Replaces the old GitHub details modal.
    /// </summary>
    [RelayCommand]
    private void OpenPullRequests(Repo? repo)
    {
        if (repo is null) return;
        _bottomBar.OpenPullRequests(repo);
    }

    /// <summary>
    /// Opens the bottom bar's Issues tab on the clicked row's repo — the issues half of
    /// what the old GitHub details modal showed.
    /// </summary>
    [RelayCommand]
    private void OpenIssues(Repo? repo)
    {
        if (repo is null) return;
        _bottomBar.OpenIssues(repo);
    }

    /// <summary>
    /// Opens the bottom bar's Changes tab on the clicked row's repo (the branch pill's
    /// action): the working-tree change list with per-file status codes.
    /// </summary>
    [RelayCommand]
    private void OpenChanges(Repo? repo)
    {
        if (repo is null) return;
        _bottomBar.OpenChanges(repo);
    }

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
    /// open work items and recent pipeline runs. Replaces the old Azure DevOps details
    /// modal.
    /// </summary>
    [RelayCommand]
    private void OpenAzureDevOpsDetails(Repo? repo)
    {
        if (repo is null) return;
        _bottomBar.OpenAzure(repo);
    }

    /// <summary>
    /// Opens zcode on the repo folder. The zcode AppImage is the Electron desktop package
    /// (it contains no interactive CLI runtime), so it is launched directly on the folder
    /// like VS Code — no terminal wrapper. A standalone zcode CLI binary has no UI of its
    /// own, so that variant still runs inside the configured terminal.
    /// </summary>
    [RelayCommand]
    private void OpenWithZCode(Repo? repo)
    {
        if (repo?.FolderPath is null) return;

        var resolved = ExecutableDefaults.Locate(_reposSettings.ZCodeExecutable)
                       ?? _reposSettings.ZCodeExecutable
                       ?? "zcode";

        if (!OperatingSystem.IsWindows() && resolved.EndsWith(".appimage", StringComparison.OrdinalIgnoreCase))
        {
            _processLauncher.StartProcess(resolved, $"\"{repo.FolderPath}\"", stripElectronEnvironment: true);
            return;
        }

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null) return;

        var zcodeExe = resolved.Contains(' ') ? $"\"{resolved}\"" : resolved;
        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, zcodeExe);
        _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
    }

    /// <summary>
    /// Resolves a CLI name for embedding in a terminal command line. The spawned
    /// terminal inherits the app's often-minimal GUI PATH, so a bare name is expanded
    /// to its absolute path; when unresolvable the bare name is kept so the terminal
    /// shows the familiar "command not found" feedback.
    /// </summary>
    private string ResolveCliForTerminal(string? configured, string fallback)
    {
        var resolved = ExecutableDefaults.Locate(configured) ?? configured ?? fallback;
        return resolved.Contains(' ') ? $"\"{resolved}\"" : resolved;
    }

    // --- OpenCode ---

    /// <summary>
    /// Quick open: launches a single opencode instance in the repo folder with the configured
    /// default model (or the first model from the list when none is configured) — no options.
    /// The cached list answers instantly; on a cold start the CLI runs once and fills the
    /// cache. The default is read live from the bottom bar so a pick made there applies
    /// immediately; the cached list always carries the default as its first entry.
    /// </summary>
    [RelayCommand]
    private async Task QuickOpenOpenCodeAsync(Repo? repo)
    {
        if (repo?.FolderPath is null || !IsOpenCodeEnabled) return;

        var defaultModel = _bottomBar.OpenCodeDefaultModel;
        var models = _openCodeModelService.GetCachedModels(defaultModel);
        if (models.Count == 0)
            models = await _openCodeModelService.GetModelsAsync(_reposSettings.OpenCodeExecutable, defaultModel);

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null) return;

        var openCodeExe = ResolveCliForTerminal(_reposSettings.OpenCodeExecutable, "opencode");
        var commandLine = OpenCodeGridLauncher.BuildCommandLine(openCodeExe, models.FirstOrDefault() ?? string.Empty, string.Empty);
        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, commandLine);
        _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
    }

    /// <summary>
    /// Opens the bottom bar's OpenCode tab on the clicked row's repo: the model picker
    /// (which persists the configured default model), instances, template, prompt and the
    /// launch button — the relocated options panel.
    /// </summary>
    [RelayCommand]
    private void OpenOpenCode(Repo? repo)
    {
        if (repo is null || !IsOpenCodeEnabled) return;
        _bottomBar.OpenOpenCode(repo);
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
            await _repoService.RefreshAsync(_reposSettings);
            // Re-check git statuses against the freshly scanned list and await them so the
            // button's busy state spans the whole cycle (scan + status). The scan itself
            // raises Changed on completion, which triggers one more status pass; awaiting
            // here coalesces both into a single IsRefreshing window.
            await _gitStatusService.RefreshAllAsync();
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
            var currentRepoSettings = settings.Repos ?? new ReposSettings();

            var edited = await _dialogService.ShowReposSettingsDialogAsync(currentRepoSettings);
            if (edited == null)
            {
                // User cancelled the dialog.
                return;
            }

            settings.Repos = edited;
            // The settings dialog doesn't touch the sort mode, but it may hand back a
            // fresh instance — carry the live selection so the save doesn't revert it.
            edited.SortMode = SelectedSortOption.Mode;
            await _settingsService.SaveSettingsAsync(settings);

            _reposSettings = edited;
            IsGitHubColumnVisible = edited.ShowGitHubColumn;
            _gitHubService.Configure(edited);
            IsAzureDevOpsColumnVisible = edited.ShowAzureDevOpsColumn;
            _azureDevOpsService.Configure(edited);
            RefreshShortcutAvailability();
            // The bottom bar's tab visibility (GitHub/Azure) and OpenCode availability
            // follow the same save.
            _bottomBar.ApplySettings(edited);
            await _repoService.RefreshAsync(_reposSettings);
            _notificationService.Show("Settings saved", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error opening repo settings");
            _notificationService.Show("Failed to save settings", NotificationKind.Error);
        }
    }
}
