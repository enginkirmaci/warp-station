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

namespace Tools.ViewModels.Pages;

/// <summary>
/// One entry of the Repos page sort selector: the sort mode plus its display label.
/// A labeled wrapper (rather than binding the raw enum) keeps the dropdown text in
/// one place and works with compiled bindings without a value converter.
/// </summary>
public sealed record RepoSortOption(RepoSortMode Mode, string Label);

/// <summary>
/// Binding adapter for the Repos page. Delegates scanning, caching, and the shared
/// repo state to <see cref="IRepoService"/> (singleton), launching to
/// <see cref="ITerminalLauncher"/> (executable resolution, argument shapes and fallback
/// decisions), and tag persistence back through the service.
/// Holds only view-specific state: the text + tag filters, the sort selection and the
/// filtered projection. The OpenCode launch panel moved to the window's bottom bar
/// (<see cref="Components.BottomBarViewModel"/>); this page keeps the per-row quick
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

    /// <summary>Both provider services behind the common contract, so the settings'
    /// Configure sweep is one loop (load and save).</summary>
    private readonly IEnumerable<IRepoActivityService> _activityServices;
    private readonly IProcessLauncher _processLauncher;
    private readonly ITerminalLauncher _terminalLauncher;
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
    private readonly UiDebounce _filterDebounce = new(FilterDebounceMs);
    private readonly UiDebounce _changedDebounce = new(ChangedDebounceMs);

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
        new RepoSortOption(RepoSortMode.PullRequests, "Pull requests"),
        new RepoSortOption(RepoSortMode.Issues, "Issues"),
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

    // --- GitHub totals (page-header summary) ---

    /// <summary>
    /// Running header totals. A repo count change folds its delta in (see
    /// <see cref="AdjustTotal"/>) instead of re-summing every repo per event — a full
    /// probe pass used to cost O(N²) enumerations of the repo set. Recomputed from
    /// scratch and re-seeded whenever the repo set may have been replaced (see
    /// <see cref="RefreshHeaderTotals"/>), so list membership changes cannot drift them.
    /// </summary>
    private int _totalPrCount;
    private int _totalIssueCount;
    private int _totalModifiedCount;

    /// <summary>
    /// Per-repo last-known contributions to the totals above: the incremental fold needs
    /// the previous value to subtract and <see cref="PropertyChangedEventArgs"/> carries
    /// none. Keyed by instance (repos are shared references, no value equality); cleared
    /// and re-seeded together with the totals.
    /// </summary>
    private readonly Dictionary<Repo, int> _prTotalContributions = new();
    private readonly Dictionary<Repo, int> _issueTotalContributions = new();
    private readonly Dictionary<Repo, int> _modifiedTotalContributions = new();

    /// <summary>
    /// Guards the totals and their contribution dictionaries: the git/GitHub probes push
    /// counts from background continuations, and a read-modify-write fold must not
    /// interleave. Getters read plain ints (atomic), so the worst case under a concurrent
    /// fold is a one-frame-stale total — which the previous per-event re-sum could show
    /// just the same.
    /// </summary>
    private readonly object _totalsLock = new();

    /// <summary>
    /// Total open pull requests across all known repos — the at-a-glance summary beside
    /// the page title. Unloaded and non-GitHub repos contribute zero. Incrementally
    /// updated when a repo's GitHub counts change (see <see
    /// cref="OnRepoPropertyChanged"/>) and recomputed after a scan replaces the repo set
    /// (see <see cref="RefreshHeaderTotals"/>).
    /// </summary>
    public int GitHubTotalPrCount => _totalPrCount;

    /// <summary>Total open issues across all known repos. See <see cref="GitHubTotalPrCount"/>.</summary>
    public int GitHubTotalIssueCount => _totalIssueCount;

    /// <summary>
    /// Whether the header summary shows at all: the GitHub column must be enabled and at
    /// least one item open across the repos — an all-zero summary is noise, matching the
    /// per-row chips that hide when their count is zero.
    /// </summary>
    public bool HasGitHubTotals => IsGitHubColumnVisible
        && (GitHubTotalPrCount > 0 || GitHubTotalIssueCount > 0);

    partial void OnIsGitHubColumnVisibleChanged(bool value)
    {
        OnPropertyChanged(nameof(HasGitHubTotals));
        OnPropertyChanged(nameof(HasHeaderStats));
    }

    /// <summary>
    /// Total uncommitted file changes across all known repos — the red third of the
    /// header stat row. Starts at zero and fills in as the background git status probes
    /// push their counts onto the entities (see <see cref="AdjustTotal"/>, wired from
    /// <see cref="OnRepoPropertyChanged"/>).
    /// </summary>
    public int GitTotalModifiedCount => _totalModifiedCount;

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

    /// <summary>
    /// Recomputes the running header totals from the current repo set in one pass and
    /// re-seeds the per-repo contributions to match, then re-raises every header total.
    /// Called after the repo set may have been replaced wholesale (initial load, rescan):
    /// the fresh entities start at zero, so previously non-zero stats must drop without
    /// any single entity carrying a change notification — and only a full recompute here
    /// keeps the incremental folds correct across list membership changes.
    /// </summary>
    private void RefreshHeaderTotals()
    {
        lock (_totalsLock)
        {
            _prTotalContributions.Clear();
            _issueTotalContributions.Clear();
            _modifiedTotalContributions.Clear();

            var pr = 0;
            var issues = 0;
            var modified = 0;
            foreach (var repo in _repoService.Repos)
            {
                pr += repo.GitHubPrCount;
                issues += repo.GitHubIssueCount;
                modified += repo.GitModifiedCount;
                _prTotalContributions[repo] = repo.GitHubPrCount;
                _issueTotalContributions[repo] = repo.GitHubIssueCount;
                _modifiedTotalContributions[repo] = repo.GitModifiedCount;
            }

            _totalPrCount = pr;
            _totalIssueCount = issues;
            _totalModifiedCount = modified;
        }

        OnPropertyChanged(nameof(GitHubTotalPrCount));
        OnPropertyChanged(nameof(GitHubTotalIssueCount));
        OnPropertyChanged(nameof(HasGitHubTotals));
        OnPropertyChanged(nameof(GitTotalModifiedCount));
        OnPropertyChanged(nameof(HasChangesTotal));
        OnPropertyChanged(nameof(HasHeaderStats));
    }

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

    public ReposViewModel(
        ISettingsService settingsService,
        IDialogService dialogService,
        IRepoService repoService,
        IGitStatusService gitStatusService,
        IGitHubService gitHubService,
        IAzureDevOpsService azureDevOpsService,
        IEnumerable<IRepoActivityService> activityServices,
        IProcessLauncher processLauncher,
        ITerminalLauncher terminalLauncher,
        IOpenCodeModelService openCodeModelService,
        INotificationService notificationService,
        BottomBarViewModel bottomBar)
    {
        _settingsService = settingsService;
        _dialogService = dialogService;
        _repoService = repoService;
        _gitStatusService = gitStatusService;
        _gitHubService = gitHubService;
        _azureDevOpsService = azureDevOpsService;
        _activityServices = activityServices;
        _processLauncher = processLauncher;
        _terminalLauncher = terminalLauncher;
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

        // Detach the live-re-sort listeners: the repos are singleton-cached and would
        // otherwise keep this Transient VM alive across navigations.
        DetachSortListeners();

        // Cancel any deferred filter/changed callbacks so a pending debounce does not fire
        // its UI-thread update after this VM is no longer the active page.
        _filterDebounce.Dispose();
        _changedDebounce.Dispose();
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
        RebuildTagFilters();
        RefreshSortListeners();
        // The repos are singleton-cached and may still carry GitHub counts / git changes
        // from an earlier page visit — seed the header totals from them (fresh loads
        // start at zero, where this raise is a harmless no-op for the UI).
        RefreshHeaderTotals();
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
        _changedDebounce.Debounce(() =>
        {
            RebuildTagFilters();
            // A rescan can replace repo instances — re-wire the live-re-sort listeners
            // to the fresh set before re-ordering, and drop the header totals the
            // orphaned entities were carrying.
            RefreshSortListeners();
            RefreshHeaderTotals();
            ApplyFilter();
        });
    }

    partial void OnFilterTextChanged(string value) => ScheduleFilterDebounce();

    /// <summary>
    /// Coalesces a burst of keystrokes into a single in-place list sync so the cards are
    /// not torn down and rebuilt per character.
    /// </summary>
    private void ScheduleFilterDebounce()
    {
        _filterDebounce.Debounce(ApplyFilter);
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

    /// <summary>
    /// Reconciles the checkable tag list with <see cref="IRepoService.AllTags"/>: reuses
    /// the existing <see cref="TagFilter"/> instance per tag name (its check state
    /// survives untouched), removes only vanished tags and adds only new ones. Most
    /// debounced scan raises find an unchanged tag set, where the merge is a no-op — the
    /// tags ItemsControl then rebuilds nothing, where the previous wholesale rebuild
    /// churned every checkbox per raise.
    /// </summary>
    private void RebuildTagFilters()
    {
        var tags = _repoService.AllTags
            .OrderBy(t => t, StringComparer.OrdinalIgnoreCase)
            .ToList();

        // Common case: same names in the same (sorted) order — keep the existing
        // instances and collection untouched.
        if (TagFilters.Count == tags.Count)
        {
            var identical = true;
            for (var i = 0; i < tags.Count; i++)
            {
                if (string.Equals(TagFilters[i].Name, tags[i], StringComparison.OrdinalIgnoreCase)) continue;
                identical = false;
                break;
            }

            if (identical) return;
        }

        var reusable = new Dictionary<string, TagFilter>(StringComparer.OrdinalIgnoreCase);
        foreach (var filter in TagFilters)
            reusable[filter.Name] = filter;

        var membershipChanged = false;
        var merged = new List<TagFilter>(tags.Count);
        foreach (var name in tags)
        {
            if (reusable.Remove(name, out var filter))
            {
                merged.Add(filter);
            }
            else
            {
                merged.Add(new TagFilter(name));
                membershipChanged = true; // a tag appeared
            }
        }

        // Whatever was not merged back belongs to a tag that vanished.
        membershipChanged |= reusable.Count > 0;

        if (membershipChanged)
        {
            // The rare case (a rescan added/removed tags): replace the collection, with
            // the surviving tags keeping their instances — and thus their check states —
            // without re-setting them.
            TagFilters = new ObservableCollection<TagFilter>(merged);
            return;
        }

        // Same tags, different order (virtually never — both sides sort the same way):
        // reorder the existing collection with Move notifications instead of replacing
        // it, so the checkboxes keep their containers.
        for (var i = 0; i < merged.Count; i++)
        {
            var target = merged[i];
            if (ReferenceEquals(TagFilters[i], target)) continue;

            for (var j = i; j < TagFilters.Count; j++)
            {
                if (!ReferenceEquals(TagFilters[j], target)) continue;
                TagFilters.Move(j, i);
                break;
            }
        }
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

        // Same hygiene for the header totals: drop the per-repo contributions so the
        // detached repos are not referenced from here anymore.
        lock (_totalsLock)
        {
            _prTotalContributions.Clear();
            _issueTotalContributions.Clear();
            _modifiedTotalContributions.Clear();
        }
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

        if (sender is not Repo repo)
        {
            return;
        }

        // The GitHub / git probes push their counts from background continuations; the
        // header totals are running sums, so each count change folds its delta in
        // (subtract this repo's previous contribution, add the new value) and raises the
        // same notifications per count kind as before. (Avalonia marshals the binding
        // updates onto the UI thread, same as the per-row chips.)
        if (e.PropertyName is nameof(Repo.GitHubPrCount))
        {
            AdjustTotal(_prTotalContributions, repo, repo.GitHubPrCount, ref _totalPrCount);
            OnPropertyChanged(nameof(GitHubTotalPrCount));
            OnPropertyChanged(nameof(HasGitHubTotals));
            OnPropertyChanged(nameof(HasHeaderStats));
        }
        else if (e.PropertyName is nameof(Repo.GitHubIssueCount))
        {
            AdjustTotal(_issueTotalContributions, repo, repo.GitHubIssueCount, ref _totalIssueCount);
            OnPropertyChanged(nameof(GitHubTotalIssueCount));
            OnPropertyChanged(nameof(HasGitHubTotals));
            OnPropertyChanged(nameof(HasHeaderStats));
        }
        else if (e.PropertyName is nameof(Repo.GitModifiedCount))
        {
            AdjustTotal(_modifiedTotalContributions, repo, repo.GitModifiedCount, ref _totalModifiedCount);
            OnPropertyChanged(nameof(GitTotalModifiedCount));
            OnPropertyChanged(nameof(HasChangesTotal));
            OnPropertyChanged(nameof(HasHeaderStats));
        }
    }

    /// <summary>
    /// Folds one repo's new count into a running header total: subtracts the repo's
    /// previous contribution (zero when never seen — a change arriving before the totals
    /// were seeded contributes its current value only) and adds the new value. The next
    /// <see cref="RefreshHeaderTotals"/> recomputes everything from the repo set, so no
    /// drift survives a rebuild.
    /// </summary>
    private void AdjustTotal(Dictionary<Repo, int> contributions, Repo repo, int value, ref int total)
    {
        lock (_totalsLock)
        {
            contributions.TryGetValue(repo, out var previous);
            total += value - previous;
            contributions[repo] = value;
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
        // Repo.IsFavorite walks the Tags collection with .Any per read, and a comparison
        // sort evaluates its primary key O(n log n) times — snapshot the flag once per
        // repo before sorting and capture the snapshot in the comparator. One tag walk
        // per repo, identical ordering (bool descending, stable, as before).
        var snapshot = repos.ToList();
        var favoriteFlags = new Dictionary<Repo, bool>(snapshot.Count);
        foreach (var repo in snapshot)
            favoriteFlags[repo] = repo.IsFavorite;

        var favoritesFirst = snapshot.OrderByDescending(r => favoriteFlags[r]);
        return SelectedSortOption.Mode switch
        {
            RepoSortMode.LastActivity => favoritesFirst
                .ThenByDescending(r => r.GitLastCommitAt)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            RepoSortMode.Changes => favoritesFirst
                .ThenByDescending(r => r.GitModifiedCount + r.GitToPushCount + r.GitToPullCount)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            RepoSortMode.PullRequests => favoritesFirst
                .ThenByDescending(r => r.GitHubPrCount)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            RepoSortMode.Issues => favoritesFirst
                .ThenByDescending(r => r.GitHubIssueCount)
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
        _terminalLauncher.OpenSolution(repo.SolutionPath, _reposSettings.IdeExecutable);
    }

    [RelayCommand]
    private void OpenFolder(string? folderPath) => _terminalLauncher.OpenFolder(folderPath);

    [RelayCommand]
    private async Task OpenWithVSCodeAsync(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;
        await _terminalLauncher.OpenInVSCodeAsync(folderPath, _reposSettings.VSCodeExecutable, _reposSettings.VSCodeProfile);
    }

    [RelayCommand]
    private void OpenWithTerminal(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;
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
        if (repo?.FolderPath is null) return;
        _terminalLauncher.OpenZCode(repo.FolderPath, _reposSettings.ZCodeExecutable, _reposSettings.TerminalExecutable);
    }

    // --- OpenCode ---

    /// <summary>
    /// Quick open: launches a single opencode instance in the repo folder with the configured
    /// default model (or the first model from the list when none is configured) — no options.
    /// The cached list answers instantly; on a cold start the CLI runs once and fills the
    /// cache. The default is read live from the bottom bar so a pick made there applies
    /// immediately. The model list is only a fallback pool — its ordering is the catalog's,
    /// never a preference, so the configured default is passed through directly rather than
    /// read back as the list's first entry.
    /// </summary>
    [RelayCommand]
    private async Task QuickOpenOpenCodeAsync(Repo? repo)
    {
        if (repo?.FolderPath is null || !IsOpenCodeEnabled) return;

        var defaultModel = _bottomBar.OpenCodeDefaultModel;
        var models = _openCodeModelService.GetCachedModels(defaultModel);
        if (models.Count == 0)
            models = await _openCodeModelService.GetModelsAsync(_reposSettings.OpenCodeExecutable, defaultModel);

        var model = _openCodeModelService.ResolveLaunchModel(models, defaultModel);

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
            IsGitHubColumnVisible = edited.EnableGitHub;
            IsAzureDevOpsColumnVisible = edited.EnableAzureDevOps;
            foreach (var activityService in _activityServices)
            {
                activityService.Configure(edited);
            }
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
