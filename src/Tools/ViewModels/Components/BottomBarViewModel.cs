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
/// git controls, GitHub/Azure panels all target it) and the expandable tab panels' data.
/// The OpenCode launch UI lives in the tool drawer (<see cref="Tools.Views.Components.OpenCodeSettingsComponent"/>);
/// this VM keeps only its integration flag/availability snapshot (which the row buttons,
/// the wand and the drawer's own seeding read) and the entry point that opens the drawer.
/// Unlike the transient drawer ViewModels this one is a singleton: it lives as long as
/// the window, so the panels' state survives page navigation.
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
    private readonly IOpenCodeRunService _openCodeRunService;
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
            FetchCommand.NotifyCanExecuteChanged();
            PullCommand.NotifyCanExecuteChanged();
            PushCommand.NotifyCanExecuteChanged();
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
        OnPropertyChanged(nameof(GitToPullCount));
        OnPropertyChanged(nameof(GitToPushCount));
    }

    /// <summary>Working-tree change count of the selected repo (the Overview card's footer).</summary>
    public int ChangesCount => SelectedRepo?.GitModifiedCount ?? 0;

    /// <summary>Behind-the-upstream count (the Changes toolbar's Pull badge); 0 with no
    /// repo selected — a flat mirror so the binding path never crosses a null
    /// SelectedRepo (that logs a binding error under a debugger on every rebind).</summary>
    public int GitToPullCount => SelectedRepo?.GitToPullCount ?? 0;

    /// <summary>Ahead-of-upstream count (the Changes toolbar's Push badge); 0 with no repo.</summary>
    public int GitToPushCount => SelectedRepo?.GitToPushCount ?? 0;

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
        BottomBarTab.Changes => LoadChangesTabAsync,
        BottomBarTab.PullRequests => LoadGitHubTabAsync,
        BottomBarTab.Issues => LoadGitHubTabAsync,
        BottomBarTab.Azure => LoadAzureAsync,
        _ => null,
    };

    /// <summary>The shared GitHub tabs' load: the GitHub lists plus — when the Azure
    /// column is on — the Azure sections shown alongside them.</summary>
    private Task LoadGitHubTabAsync()
    {
        _ = LoadGitHubAsync();
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

    /// <summary>Refreshes the open panel's data (the header's refresh button).</summary>
    [RelayCommand]
    private void RefreshPanel() => ReloadActiveTab();

    // --- Shared panel loader plumbing ---
    // Every panel load repeats the same mechanics: snapshot the selected repo, fetch,
    // drop the result when the repo switched mid-load, swap the list under a busy flag
    // and raise the derived bindings. These helpers are that idiom, factored out once;
    // the GitHub/Azure twins additionally share the cache-seed and refresh skeletons
    // (<see cref="LoadProviderAsync{TActivity}"/>, <see cref="RefreshProviderAsync{TActivity}"/>).

    /// <summary>
    /// Fetches a payload for the snapshotted repo and hands it back only when the user
    /// has not switched repos while the fetch ran — the "repo switched mid-load" guard
    /// every panel loader repeats. Returns null when the load was abandoned mid-flight
    /// (the caller then skips its state updates).
    /// </summary>
    private async Task<T?> FetchIfCurrentAsync<T>(Repo repo, Func<Repo, Task<T>> fetch) where T : class
    {
        var payload = await fetch(repo);
        if (!ReferenceEquals(SelectedRepo, repo)) return default; // repo switched while loading
        return payload;
    }

    /// <summary>Rebuilds a bound collection in place (Clear + Add) — the loaders' swap
    /// step, which keeps the collection instance (and its bindings) alive.</summary>
    private static void ReplaceItems<T>(ObservableCollection<T> target, IEnumerable<T> items)
    {
        target.Clear();
        foreach (var item in items)
        {
            target.Add(item);
        }
    }

    /// <summary>Runs a panel load under its busy flag: raises it up front, restores it
    /// in finally and pushes the flag-derived bindings — the try/finally shape every
    /// loader repeated. Exceptions keep propagating, as before.</summary>
    private async Task RunBusyAsync(Action<bool> setBusy, Func<Task> load, Action raiseSettled)
    {
        setBusy(true);
        try
        {
            await load();
        }
        finally
        {
            setBusy(false);
            raiseSettled();
        }
    }

    /// <summary>
    /// The GitHub/Azure load twin: seed the panel from the service's cache so opening
    /// the tab is instant, then run the refresh that replaces the lists when it lands.
    /// The caller owns the null-repo branch (each panel clears its own collections).
    /// </summary>
    private async Task LoadProviderAsync<TActivity>(
        Func<TActivity?> getCached,
        Action<TActivity> apply,
        Func<Task> refresh)
    {
        var cached = getCached();
        if (cached is not null)
        {
            apply(cached);
        }

        await refresh();
    }

    /// <summary>
    /// The GitHub/Azure refresh twin: fetches the provider's activity for the selected
    /// repo under the panel's busy flag, keeps the previous lists when the repo switched
    /// mid-load or the fetch failed (logged with the provider label — a failed fetch
    /// returns an empty activity, and applying it would flash a misleading all-clear),
    /// surfaces the provider's availability note and applies the activity. A no-op when
    /// there is no repo or <paramref name="canStart"/> refuses (the Azure panel never
    /// overlaps its own refresh; the GitHub one relies on its command's CanExecute).
    /// </summary>
    private async Task RefreshProviderAsync<TActivity>(
        string providerLabel,
        Func<bool> canStart,
        Action<bool> setRefreshing,
        Func<Repo, Task<TActivity>> fetch,
        Action<Repo> setUnavailable,
        Action<TActivity> apply) where TActivity : class
    {
        var repo = SelectedRepo;
        if (repo is null || !canStart()) return;

        setRefreshing(true);
        try
        {
            var activity = await FetchIfCurrentAsync(repo, fetch);
            if (activity is null) return; // repo switched while loading
            setUnavailable(repo);
            apply(activity);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "{Provider} panel refresh failed for {FolderPath}", providerLabel, repo.FolderPath);
        }
        finally
        {
            setRefreshing(false);
        }
    }

    // --- Git commands' shared plumbing ---
    // The bar's git commands repeat one shape: run the service call under the command's
    // busy flag, toast Success/Error on the outcome and run the per-command follow-ups
    // (branch re-sync, tab reload). RunGitActionAsync is that shape, factored out once.

    /// <summary>
    /// Runs one git service call under <paramref name="setBusy"/> (null when the command
    /// has no busy flag), toasts <paramref name="successText"/> on success and
    /// <paramref name="errorText"/> on failure (null skips a toast; the error lambda runs
    /// after the action, so fetch/pull/push can embed git's stderr line) and invokes the
    /// outcome hooks. Exceptions keep propagating, exactly as before.
    /// </summary>
    private async Task RunGitActionAsync(
        Repo repo,
        Action<bool>? setBusy,
        Func<Repo, Task<bool>> action,
        string? successText,
        Func<string?> errorText,
        Action? onSuccess = null,
        Action? onFailure = null)
    {
        setBusy?.Invoke(true);
        try
        {
            if (await action(repo))
            {
                if (successText is not null)
                {
                    _notificationService.Show(successText, NotificationKind.Success);
                }

                onSuccess?.Invoke();
            }
            else
            {
                if (errorText() is { } error)
                {
                    _notificationService.Show(error, NotificationKind.Error);
                }

                onFailure?.Invoke();
            }
        }
        finally
        {
            setBusy?.Invoke(false);
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

    private Task CheckoutAsync(string branch)
    {
        var repo = SelectedRepo;
        if (repo is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            value => IsCheckingOut = value,
            r => _gitStatusService.CheckoutAsync(r, branch),
            $"Checked out {branch} in {repo.Name}",
            () => $"Checkout of {branch} failed",
            onSuccess: () =>
            {
                SyncBranchSelection(repo);
                _ = LoadBranchesAsync();
            },
            onFailure: () => SyncBranchSelection(repo));
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
    private Task FetchAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return Task.CompletedTask;

        string? error = null;
        return RunGitActionAsync(
            repo,
            value => IsFetching = value,
            async r =>
            {
                var result = await _gitStatusService.FetchAsync(r);
                error = result.Error;
                return result.Success;
            },
            $"Fetched {repo.Name}",
            () => error is { } detail
                ? $"Fetch failed for {repo.Name}: {detail}"
                : $"Fetch failed for {repo.Name}",
            onSuccess: () =>
            {
                SyncBranchSelection(repo);
                RefreshGitCounts(repo);
            });
    }

    private bool CanFetch() => !IsFetching && HasSelectedRepo;

    /// <summary>
    /// Re-probes the repo's git status right after a pull/push/fetch: the ahead/behind
    /// counts the Pull/Push buttons display must reflect the operation that just ran
    /// (pull clears behind, push clears ahead, fetch may reveal new upstream commits)
    /// without waiting for the next full status pass. Fire-and-forget — the probe is
    /// all-swallowing and updates the repo entity's observable counts.
    /// </summary>
    private void RefreshGitCounts(Repo repo) => _ = _gitStatusService.RefreshRepoAsync(repo);

    [RelayCommand(CanExecute = nameof(CanPull))]
    private Task PullAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return Task.CompletedTask;

        string? error = null;
        return RunGitActionAsync(
            repo,
            value => IsPulling = value,
            async r =>
            {
                var result = await _gitStatusService.PullAsync(r);
                error = result.Error;
                return result.Success;
            },
            $"Pulled {repo.Name}",
            () => error is { } detail
                ? $"Pull failed for {repo.Name}: {detail}"
                : $"Pull failed for {repo.Name}",
            onSuccess: () =>
            {
                SyncBranchSelection(repo);
                RefreshGitCounts(repo);
            });
    }

    private bool CanPull() => !IsPulling && !IsPushing && HasSelectedRepo;

    [RelayCommand(CanExecute = nameof(CanPush))]
    private Task PushAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return Task.CompletedTask;

        string? error = null;
        return RunGitActionAsync(
            repo,
            value => IsPushing = value,
            async r =>
            {
                var result = await _gitStatusService.PushAsync(r);
                error = result.Error;
                return result.Success;
            },
            $"Pushed {repo.Name}",
            () => error is { } detail
                ? $"Push failed for {repo.Name}: {detail}"
                : $"Push failed for {repo.Name}",
            onSuccess: () => RefreshGitCounts(repo));
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

    /// <summary>First five changed files for the Overview card (the Changes tab lists all).
    /// A cached slice — recomputed only when the list (re)fills, instead of re-enumerating
    /// <see cref="ChangedFiles"/> on every binding evaluation.</summary>
    public IReadOnlyList<GitChangedFile> ChangedFilesPreview => _changedFilesPreview;

    private IReadOnlyList<GitChangedFile> _changedFilesPreview = Array.Empty<GitChangedFile>();

    /// <summary>Recomputes the Overview card's changed-files slice and raises its binding —
    /// called wherever <see cref="ChangedFiles"/> is (re)filled or cleared.</summary>
    private void RefreshChangedFilesPreview()
    {
        _changedFilesPreview = ChangedFiles.Take(5).ToList();
        OnPropertyChanged(nameof(ChangedFilesPreview));
    }

    partial void OnIsLoadingFilesChanged(bool value) => OnPropertyChanged(nameof(ShowChangesEmpty));

    partial void OnChangesAdditionsChanged(int value) => OnPropertyChanged(nameof(ChangesDeltaText));

    partial void OnChangesDeletionsChanged(int value) => OnPropertyChanged(nameof(ChangesDeltaText));

    private Task LoadChangedFilesAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            ChangedFiles.Clear();
            ChangesAdditions = 0;
            ChangesDeletions = 0;
            OnPropertyChanged(nameof(ShowChangesEmpty));
            RefreshChangedFilesPreview();
            return Task.CompletedTask;
        }

        return RunBusyAsync(
            value => IsLoadingFiles = value,
            async () =>
            {
                var files = await FetchIfCurrentAsync(repo, r => _gitStatusService.GetChangedFilesAsync(r));
                if (files is null) return;
                ReplaceItems(ChangedFiles, files);
                // Sum locally and assign: the observable totals are never reset between
                // loads, so accumulating on them would compound with every reload.
                ChangesAdditions = files.Sum(f => f.Additions ?? 0);
                ChangesDeletions = files.Sum(f => f.Deletions ?? 0);
            },
            () =>
            {
                OnPropertyChanged(nameof(ShowChangesEmpty));
                RefreshChangedFilesPreview();
            });
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

    private Task LoadChangeGroupsAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            StagedFiles.Clear();
            UnstagedFiles.Clear();
            IsLoadingGroups = false;
            RaiseChangeGroupsDerived();
            return Task.CompletedTask;
        }

        return RunBusyAsync(
            value => IsLoadingGroups = value,
            async () =>
            {
                var groups = await FetchIfCurrentAsync(repo, r => _gitStatusService.GetChangeGroupsAsync(r));
                if (groups is null) return;
                ReplaceItems(StagedFiles, groups.Staged);
                ReplaceItems(UnstagedFiles, groups.Unstaged);
            },
            () => RaiseChangeGroupsDerived());
    }

    /// <summary>Stages one file (+ button on an unstaged row).</summary>
    [RelayCommand]
    private Task StageFileAsync(GitChangedFile? file)
    {
        var repo = SelectedRepo;
        if (repo is null || file is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            setBusy: null,
            r => _gitStatusService.StageAsync(r, file.Path),
            successText: null,
            errorText: () => $"Could not stage {file.Path}",
            onSuccess: () => _ = LoadChangesTabAsync());
    }

    /// <summary>Unstages one file (− button on a staged row); the working tree keeps the change.</summary>
    [RelayCommand]
    private Task UnstageFileAsync(GitChangedFile? file)
    {
        var repo = SelectedRepo;
        if (repo is null || file is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            setBusy: null,
            r => _gitStatusService.UnstageAsync(r, file.Path),
            successText: null,
            errorText: () => $"Could not unstage {file.Path}",
            onSuccess: () => _ = LoadChangesTabAsync());
    }

    /// <summary>Stages everything, untracked files and deletions included (Stage All).</summary>
    [RelayCommand]
    private Task StageAllAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            setBusy: null,
            r => _gitStatusService.StageAllAsync(r),
            successText: null,
            errorText: () => $"Could not stage the changes of {repo.Name}",
            onSuccess: () => _ = LoadChangesTabAsync());
    }

    /// <summary>Unstages everything — index back to HEAD, working tree untouched (Unstage All).</summary>
    [RelayCommand]
    private Task UnstageAllAsync()
    {
        var repo = SelectedRepo;
        if (repo is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            setBusy: null,
            r => _gitStatusService.UnstageAllAsync(r),
            successText: null,
            errorText: () => $"Could not unstage the changes of {repo.Name}",
            onSuccess: () => _ = LoadChangesTabAsync());
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

        await _clipboardService.CopyTextAsync(commit.Hash);
        _notificationService.Show($"Copied {commit.ShortHash} to clipboard", NotificationKind.Success);
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
    private Task LoadRecentCommitsAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            GitCommits.Clear();
            OnPropertyChanged(nameof(ShowCommitsEmpty));
            return Task.CompletedTask;
        }

        return RunBusyAsync(
            value => IsLoadingCommits = value,
            async () =>
            {
                var commits = await FetchIfCurrentAsync(repo, r => _gitStatusService.GetRecentCommitsAsync(r));
                if (commits is null) return;
                ReplaceItems(GitCommits, commits);
            },
            () => OnPropertyChanged(nameof(ShowCommitsEmpty)));
    }

    // --- GitHub tabs (pull requests + issues) ---

    /// <summary>Whether the GitHub tabs show at all (mirrors the GitHub column setting).</summary>
    [ObservableProperty]
    private bool _isGitHubEnabled;

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _gitHubPullRequests = new();

    [ObservableProperty]
    private ObservableCollection<GitHubItem> _gitHubIssues = new();

    /// <summary>First five pull requests / issues for the Overview cards (tabs list all).
    /// Cached slices — recomputed only when the lists (re)fill, instead of re-enumerating
    /// the collections on every binding evaluation.</summary>
    public IReadOnlyList<GitHubItem> GitHubPullRequestsPreview => _gitHubPullRequestsPreview;

    public IReadOnlyList<GitHubItem> GitHubIssuesPreview => _gitHubIssuesPreview;

    private IReadOnlyList<GitHubItem> _gitHubPullRequestsPreview = Array.Empty<GitHubItem>();

    private IReadOnlyList<GitHubItem> _gitHubIssuesPreview = Array.Empty<GitHubItem>();

    /// <summary>Tab header totals: GitHub items plus the Azure DevOps ones shown in the
    /// same tabs' Azure sections.</summary>
    public int OpenPullRequestCount => GitHubPullRequests.Count + AzurePullRequests.Count;

    public int OpenIssueCount => GitHubIssues.Count + AzureWorkItems.Count;

    /// <summary>Recomputes the Overview cards' GitHub preview slices and raises their
    /// bindings — called wherever the GitHub lists are (re)filled or cleared.</summary>
    private void RefreshGitHubPreviews()
    {
        _gitHubPullRequestsPreview = GitHubPullRequests.Take(5).ToList();
        _gitHubIssuesPreview = GitHubIssues.Take(5).ToList();
        OnPropertyChanged(nameof(GitHubPullRequestsPreview));
        OnPropertyChanged(nameof(GitHubIssuesPreview));
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

    private Task LoadGitHubAsync()
    {
        var repo = SelectedRepo;
        if (repo is null)
        {
            GitHubPullRequests.Clear();
            GitHubIssues.Clear();
            RefreshGitHubPreviews();
            return Task.CompletedTask;
        }

        return LoadProviderAsync(
            () => _gitHubService.GetCachedActivity(repo),
            ApplyGitHubActivity,
            RefreshGitHubAsync);
    }

    /// <summary>Refreshes the GitHub pull requests and issues from github.com.</summary>
    [RelayCommand(CanExecute = nameof(CanRefreshGitHub))]
    private Task RefreshGitHubAsync() => RefreshProviderAsync(
        "GitHub",
        () => true,
        value => IsGitHubRefreshing = value,
        repo => _gitHubService.RefreshRepoAsync(repo),
        repo => GitHubIsUnavailable = !repo.GitHubAvailable,
        ApplyGitHubActivity);

    private bool CanRefreshGitHub() => !IsGitHubRefreshing;

    private void ApplyGitHubActivity(GitHubActivity activity)
    {
        // In-place sync: the same collection instances keep serving the ItemsControls,
        // so a refresh recycles rows instead of regenerating every container.
        ReplaceItems(GitHubPullRequests, activity.PullRequests);
        ReplaceItems(GitHubIssues, activity.Issues);
        GitHubHasLoaded = true;
        RefreshGitHubPreviews();
        OnPropertyChanged(nameof(OpenPullRequestCount));
        OnPropertyChanged(nameof(OpenIssueCount));
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

    /// <summary>First three Azure items for the Overview cards' Azure sections. Cached
    /// slices — recomputed only when the lists (re)fill, instead of re-enumerating
    /// the collections on every binding evaluation.</summary>
    public IReadOnlyList<AzureDevOpsItem> AzurePullRequestsPreview => _azurePullRequestsPreview;

    public IReadOnlyList<AzureDevOpsItem> AzureWorkItemsPreview => _azureWorkItemsPreview;

    private IReadOnlyList<AzureDevOpsItem> _azurePullRequestsPreview = Array.Empty<AzureDevOpsItem>();

    private IReadOnlyList<AzureDevOpsItem> _azureWorkItemsPreview = Array.Empty<AzureDevOpsItem>();

    /// <summary>Recomputes the Overview cards' Azure preview slices and raises their
    /// bindings — called wherever the Azure lists are (re)filled or cleared.</summary>
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

    private Task LoadAzureAsync()
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
            RefreshAzurePreviews();
            OnPropertyChanged(nameof(OpenPullRequestCount));
            OnPropertyChanged(nameof(OpenIssueCount));
            OnPropertyChanged(nameof(ShowPullRequestsEmpty));
            OnPropertyChanged(nameof(ShowIssuesEmpty));
            RaisePipelineStatus();
            return Task.CompletedTask;
        }

        return LoadProviderAsync(
            () => _azureDevOpsService.GetCachedActivity(repo),
            ApplyAzureActivity,
            RefreshAzureAsync);
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

    /// <summary>Refreshes the Azure pull requests, work items and pipeline runs.</summary>
    [RelayCommand(CanExecute = nameof(CanRefreshAzure))]
    private Task RefreshAzureAsync() => RefreshProviderAsync(
        "Azure",
        () => !IsAzureRefreshing,
        value => IsAzureRefreshing = value,
        repo => _azureDevOpsService.RefreshRepoAsync(repo),
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
        OnPropertyChanged(nameof(HasAzurePullRequests));
        OnPropertyChanged(nameof(HasAzureWorkItems));
        OnPropertyChanged(nameof(HasAzurePipelineRuns));
        OnPropertyChanged(nameof(OpenPullRequestCount));
        OnPropertyChanged(nameof(OpenIssueCount));
        OnPropertyChanged(nameof(ShowPullRequestsEmpty));
        OnPropertyChanged(nameof(ShowIssuesEmpty));
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
}
