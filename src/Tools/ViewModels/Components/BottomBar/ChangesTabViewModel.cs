using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Helpers;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.ViewModels.Windows;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// One section header row inside the Changes list's merged rows
/// (<see cref="ChangesTabViewModel.ChangeRows"/>): the section title + live count on
/// the left, the Stage All / Unstage All text button on the right. The side rides
/// as a flag so the single row template can branch icon, count and button.
/// </summary>
public sealed record ChangeSectionRow(string Title, int Count, bool IsUnstaged);

/// <summary>
/// The branch dropdown's first entry — an action, not a branch: picking it opens the
/// new-branch drawer (the sidebar component) instead of checking anything out. Rides
/// in <see cref="ChangesTabViewModel.BranchMenuItems"/> ahead of the real branches so
/// the stock ComboBox renders it as a row like any other.
/// </summary>
public sealed record NewBranchEntry
{
    /// <summary>The shared instance the dropdown menu carries.</summary>
    public static readonly NewBranchEntry Instance = new();
}

/// <summary>
/// The Changes tab (commit workspace): the staged/unstaged tree as one virtualized
/// merged list, the staging buttons, the commit message box (with the opencode
/// wand) and the git rail beside it — a menu of git actions (fetch, pull, push,
/// sync, discard-all, history toggle) over the branch dropdown, with the
/// last-fetched status pinned to the rail's bottom edge. Hangs off the bar shell
/// — the selected repo lives there, the tab loads everything it shows for it.
/// </summary>
public partial class ChangesTabViewModel : BottomBarPanelViewModel
{
    private readonly CommitMessageGenerator _messageGenerator;

    /// <summary>True while the branch ComboBox is being synced programmatically (repo
    /// switch, checkout completion) so the selection change doesn't re-run a checkout.</summary>
    private bool _updatingBranchSelection;

    public ChangesTabViewModel(BottomBarViewModel shell, CommitMessageGenerator messageGenerator)
        : base(shell)
    {
        _messageGenerator = messageGenerator;
    }

// --- Repo-derived mirrors ---
// Flat mirrors of the selected repo's state so the rail's binding paths never
// cross a null SelectedRepo (that logs a binding error under a debugger on every
// rebind). The shell raises these whenever the repo or its live counters change.

/// <summary>Whether a repo is selected (gates fetch and the commit paths).</summary>
public bool HasSelectedRepo => Shell.SelectedRepo is not null;

/// <summary>Behind-the-upstream count (the Pull button's badge); 0 with no repo.</summary>
public int GitToPullCount => Shell.SelectedRepo?.GitToPullCount ?? 0;

/// <summary>Ahead-of-upstream count (the Push button's badge); 0 with no repo.</summary>
public int GitToPushCount => Shell.SelectedRepo?.GitToPushCount ?? 0;

/// <summary>"Last fetched: 2m ago", or null when the repo was never fetched.</summary>
public string? LastFetchText => Shell.SelectedRepo?.GitLastFetchLabel is { } label ? $"Last fetched: {label}" : null;

public bool HasFetched => Shell.SelectedRepo?.GitLastFetchAt is not null;

/// <summary>Re-raises the mirrors — the shell calls this whenever the selected repo
/// is (re)assigned or one of its watched properties changes underneath it.</summary>
public void RaiseRepoMirrors()
{
    OnPropertyChanged(nameof(HasSelectedRepo));
    OnPropertyChanged(nameof(GitToPullCount));
    OnPropertyChanged(nameof(GitToPushCount));
    OnPropertyChanged(nameof(HasFetched));
    OnPropertyChanged(nameof(LastFetchText));
}

    /// <summary>
    /// The observed repo changed (a pick from the table, a rescan re-resolve, or the
    /// bar closing): a REAL switch abandons the previous repo's staged set — an
    /// in-flight message generation for it is now pointless, and its message must not
    /// linger in the box where the next Commit would send it to the new repo.
    /// </summary>
    public void OnObservedRepoChanged(bool isRealSwitch)
    {
        if (isRealSwitch)
        {
            _messageGenerator.Cancel();
            CommitMessage = string.Empty;

            // An open History view must not outlive its repo: drop the old rows
            // and reload for the new one (an empty pass is fine — the note shows).
            GitCommits.Clear();
            OnPropertyChanged(nameof(ShowCommitsEmpty));
            if (ShowHistoryView)
            {
                _ = LoadRecentCommitsAsync();
            }
        }

        // CanExecute inputs the generators cannot hook (computed, not ObservableProperty).
        FetchCommand.NotifyCanExecuteChanged();
        PullCommand.NotifyCanExecuteChanged();
        PushCommand.NotifyCanExecuteChanged();
        SyncCommand.NotifyCanExecuteChanged();
        DiscardAllCommand.NotifyCanExecuteChanged();
    }

    // --- Git rail: branch dropdown, checkout, pull/push/fetch ---

    /// <summary>The branch dropdown's menu: the New branch entry, the selected repo's
    /// local branches, a "Remote" group label and the remote-tracking branches. Rebuilt
    /// by <see cref="LoadBranchesAsync"/>.</summary>
    [ObservableProperty]
    private ObservableCollection<object> _branchMenuItems = new();

    /// <summary>The dropdown's current selection: a branch row after a repo switch /
    /// checkout sync, or the <see cref="NewBranchEntry"/> sentinel while the user's pick
    /// of it is being handled. See <see cref="OnSelectedMenuItemChanged"/>.</summary>
    [ObservableProperty]
    private object? _selectedMenuItem;

    /// <summary>The selected branch in display form — the dropdown's tooltip. The
    /// New branch sentinel and the Remote label are not branches, so they read as null.</summary>
    public string? SelectedBranch => (SelectedMenuItem as GitBranchRef)?.Name;

    /// <summary>True while a checkout is running; disables the branch dropdown.</summary>
    [ObservableProperty]
    private bool _isCheckingOut;

    /// <summary>True while a fetch is running; disables the rail's refresh button.</summary>
    [ObservableProperty]
    private bool _isFetching;

    /// <summary>True while a pull is running; disables the Pull button.</summary>
    [ObservableProperty]
    private bool _isPulling;

    /// <summary>True while a push is running; disables the Push row.</summary>
    [ObservableProperty]
    private bool _isPushing;

    /// <summary>True while a sync (pull then push) is running; disables the Sync row.</summary>
    [ObservableProperty]
    private bool _isSyncing;

    /// <summary>True while discarding all changes is running; disables the discard row.</summary>
    [ObservableProperty]
    private bool _isDiscarding;

    /// <summary>Pull, push and sync exclude each other — concurrent syncs of one repo interleave badly.</summary>
    partial void OnIsPullingChanged(bool value)
    {
        PullCommand.NotifyCanExecuteChanged();
        PushCommand.NotifyCanExecuteChanged();
        SyncCommand.NotifyCanExecuteChanged();
    }

    partial void OnIsPushingChanged(bool value)
    {
        PullCommand.NotifyCanExecuteChanged();
        PushCommand.NotifyCanExecuteChanged();
        SyncCommand.NotifyCanExecuteChanged();
    }

    partial void OnIsSyncingChanged(bool value)
    {
        PullCommand.NotifyCanExecuteChanged();
        PushCommand.NotifyCanExecuteChanged();
        SyncCommand.NotifyCanExecuteChanged();
    }

    partial void OnIsDiscardingChanged(bool value) => DiscardAllCommand.NotifyCanExecuteChanged();

    /// <summary>
    /// The branch dropdown is active: picking a branch checks it out in the selected
    /// repo (a remote pick checks out its local tracking equivalent); picking the New
    /// branch entry opens the new-branch drawer and snaps the dropdown back to the
    /// current branch (the entry is an action, not a selection) — same snap-back for
    /// the Remote group label.
    /// Programmatic syncs (repo switch, checkout completion) pass through the
    /// <see cref="_updatingBranchSelection"/> guard; a failed checkout reverts the
    /// dropdown to the repo's actual branch.
    /// </summary>
    partial void OnSelectedMenuItemChanged(object? value)
    {
        OnPropertyChanged(nameof(SelectedBranch));
        if (_updatingBranchSelection || IsCheckingOut) return;

        if (value is NewBranchEntry)
        {
            OpenNewBranchDrawer();
            return;
        }

        if (value is not GitBranchRef branch) return;
        if (branch.IsHeader)
        {
            SyncBranchSelection(Repo); // the label is not a branch — never leave it selected
            return;
        }

        if (Repo is null) return;
        if (branch.IsLocal && string.Equals(branch.Name, Repo.GitBranchName, StringComparison.Ordinal)) return;
        _ = CheckoutAsync(branch);
    }

    /// <summary>
    /// Opens the new-branch drawer on the selected repo, seeding it with the settings'
    /// branch-name prefix (pre-typed into the drawer's name field) and the hook that
    /// re-syncs this dropdown once the branch landed (and was checked out).
    /// </summary>
    private void OpenNewBranchDrawer()
    {
        var repo = Repo;
        SyncBranchSelection(repo); // the entry is an action — never leave it selected
        if (repo is null) return;

        Shell.Drawers.Open(
            ToolComponentMapper.NewBranchKey,
            new NewBranchContext(
                repo,
                Shell.ReposSettings.BranchNamePrefix,
                OnCreated: () =>
                {
                    SyncBranchSelection(repo);
                    _ = LoadBranchesAsync();
                }));
    }

    private Task CheckoutAsync(GitBranchRef branch)
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        // A remote pick checks out its local tracking equivalent: plain `git checkout
        // <short>` creates a tracking local branch when none exists (DWIM), or switches
        // to the existing local one. A local pick is its own target.
        var target = branch.IsRemote ? branch.Name[(branch.Name.IndexOf('/') + 1)..] : branch.Name;

        return RunGitActionAsync(
            repo,
            value => IsCheckingOut = value,
            r => Shell.GitStatusService.CheckoutAsync(r, target),
            $"Checked out {target} in {repo.Name}",
            () => $"Checkout of {target} failed",
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
    public async Task LoadBranchesAsync()
    {
        var repo = Repo;
        if (repo is null)
        {
            BranchMenuItems.Clear();
            SyncBranchSelection(null);
            return;
        }

        try
        {
            var branches = await Shell.GitStatusService.GetBranchesAsync(repo);
            if (!ReferenceEquals(Shell.SelectedRepo, repo)) return; // repo switched while loading

            // Clear() resets the ComboBox's selection, and re-assigning an unchanged
            // SelectedMenuItem value afterwards raises no change — the placeholder would
            // stick. Rebuild only when the menu really changed (the New branch entry and
            // the Remote label are derived rows, so the whole menu shape compares), and
            // drop the stale selection first (guarded: the null must not read as a user
            // checkout pick).
            var menu = BuildBranchMenu(branches);
            if (!BranchMenuItems.SequenceEqual(menu))
            {
                BranchMenuItems.Clear();
                foreach (var item in menu)
                {
                    BranchMenuItems.Add(item);
                }
                _updatingBranchSelection = true;
                try
                {
                    SelectedMenuItem = null;
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
            Shell.Notifications.Show($"Could not load the branches of {repo.Name}", NotificationKind.Error);
        }
    }

    /// <summary>
    /// Shapes the service's branch rows into the dropdown menu: the New branch entry
    /// rides ahead, and a non-selectable "Remote" label separates the local group from
    /// the remote-tracking group (absent when the repo has no remotes fetched).
    /// </summary>
    private static List<object> BuildBranchMenu(IReadOnlyList<GitBranchRef> branches)
    {
        List<object> menu = [NewBranchEntry.Instance, .. branches];
        var firstRemote = menu.FindIndex(o => o is GitBranchRef { IsRemote: true });
        if (firstRemote >= 0)
        {
            menu.Insert(firstRemote, new GitBranchRef("Remote", GitBranchKind.Header));
        }
        return menu;
    }

    private void SyncBranchSelection(Repo? repo)
    {
        _updatingBranchSelection = true;
        try
        {
            SelectedMenuItem = repo?.GitBranchName is { } name
                ? BranchMenuItems.OfType<GitBranchRef>().FirstOrDefault(b => b.IsLocal && b.Name == name)
                : null;
        }
        finally
        {
            _updatingBranchSelection = false;
        }
    }

    [RelayCommand(CanExecute = nameof(CanFetch))]
    private Task FetchAsync()
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        string? error = null;
        return RunGitActionAsync(
            repo,
            value => IsFetching = value,
            async r =>
            {
                var result = await Shell.GitStatusService.FetchAsync(r);
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
                _ = LoadBranchesAsync(); // a fetch can land new remote-tracking branches
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
    private void RefreshGitCounts(Repo repo) => _ = Shell.GitStatusService.RefreshRepoAsync(repo);

    [RelayCommand(CanExecute = nameof(CanPull))]
    private Task PullAsync()
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        string? error = null;
        return RunGitActionAsync(
            repo,
            value => IsPulling = value,
            async r =>
            {
                var result = await Shell.GitStatusService.PullAsync(r);
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

    private bool CanPull() => !IsPulling && !IsPushing && !IsSyncing && HasSelectedRepo;

    [RelayCommand(CanExecute = nameof(CanPush))]
    private Task PushAsync()
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        string? error = null;
        return RunGitActionAsync(
            repo,
            value => IsPushing = value,
            async r =>
            {
                var result = await Shell.GitStatusService.PushAsync(r);
                error = result.Error;
                return result.Success;
            },
            $"Pushed {repo.Name}",
            () => error is { } detail
                ? $"Push failed for {repo.Name}: {detail}"
                : $"Push failed for {repo.Name}",
            onSuccess: () => RefreshGitCounts(repo));
    }

    private bool CanPush() => !IsPulling && !IsPushing && !IsSyncing && HasSelectedRepo;

    /// <summary>
    /// Syncs the branch with its upstream: pull first (fast-forward when possible),
    /// push after — a failed pull skips the push, and the first error detail wins the
    /// toast. A pull that leaves conflicts fails here like any other pull failure.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanSync))]
    private Task SyncAsync()
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        string? error = null;
        return RunGitActionAsync(
            repo,
            value => IsSyncing = value,
            async r =>
            {
                var pull = await Shell.GitStatusService.PullAsync(r);
                if (!pull.Success)
                {
                    error = pull.Error;
                    return false;
                }

                var push = await Shell.GitStatusService.PushAsync(r);
                if (!push.Success)
                {
                    error = push.Error;
                    return false;
                }

                return true;
            },
            $"Synced {repo.Name}",
            () => error is { } detail
                ? $"Sync failed for {repo.Name}: {detail}"
                : $"Sync failed for {repo.Name}",
            onSuccess: () =>
            {
                SyncBranchSelection(repo);
                RefreshGitCounts(repo);
            });
    }

    private bool CanSync() => !IsPulling && !IsPushing && !IsSyncing && HasSelectedRepo;

    /// <summary>
    /// Discards EVERY change in the working tree: <c>reset --hard</c> drops the staged
    /// and unstaged edits on tracked files, <c>clean -fd</c> deletes untracked files
    /// and folders — there is no undo, so the row only enables while there is
    /// something to discard. Reloads the list on success (both sections empty).
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanDiscardAll))]
    private Task DiscardAllAsync()
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            value => IsDiscarding = value,
            async r => (await Shell.GitStatusService.DiscardAllAsync(r)).Success,
            $"Discarded all changes in {repo.Name}",
            () => $"Could not discard the changes of {repo.Name}",
            onSuccess: () => _ = LoadTabAsync());
    }

    private bool CanDiscardAll() => !IsDiscarding && HasSelectedRepo
        && (StagedFiles.Count > 0 || UnstagedFiles.Count > 0);

    /// <summary>
    /// Runs one git service call under <paramref name="setBusy"/> (null when the command
    /// has no busy flag) with the shared outcome toasting (<see cref="GitToasts"/> —
    /// success/failure text, exceptions treated as failed outcomes) plus this workspace's
    /// follow-up hooks (branch re-sync, tab reload).
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
            var ok = await GitToasts.RunAsync(
                Shell.Notifications,
                () => action(repo),
                () => successText,
                errorText,
                onFailure,
                logContext: repo.FolderPath);
            if (ok)
            {
                onSuccess?.Invoke();
            }
        }
        finally
        {
            setBusy?.Invoke(false);
        }
    }

    // --- Changed files ---

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

    /// <summary>Whether the Changes tab's lists are still loading (drives the card's
    /// loading note, so the card shows feedback instead of a blank surface).</summary>
    public bool IsChangesTabLoading => IsLoadingFiles || IsLoadingGroups;

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

    partial void OnIsLoadingFilesChanged(bool value)
    {
        OnPropertyChanged(nameof(ShowChangesEmpty));
        OnPropertyChanged(nameof(IsChangesTabLoading));
    }

    partial void OnChangesAdditionsChanged(int value) => OnPropertyChanged(nameof(ChangesDeltaText));

    partial void OnChangesDeletionsChanged(int value) => OnPropertyChanged(nameof(ChangesDeltaText));

    public Task LoadChangedFilesAsync()
    {
        var repo = Repo;
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
                var files = await FetchIfCurrentAsync(repo, r => Shell.GitStatusService.GetChangedFilesAsync(r));
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
            },
            () => $"Could not load the changes of {repo.Name}");
    }

    /// <summary>
    /// Loads everything the Changes tab shows: the merged change list (keeps the
    /// Overview card's preview fresh) and the staged/unstaged split. Concurrent.
    /// </summary>
    public Task LoadTabAsync()
    {
        _ = LoadChangedFilesAsync();
        _ = LoadChangeGroupsAsync();
        return Task.CompletedTask;
    }

    // --- Staged/unstaged split ---

    /// <summary>The index side of the selected repo's changes (the Staged section).</summary>
    [ObservableProperty]
    private ObservableCollection<GitChangedFile> _stagedFiles = new();

    /// <summary>The worktree side (the Unstaged section; untracked files included).</summary>
    [ObservableProperty]
    private ObservableCollection<GitChangedFile> _unstagedFiles = new();

    /// <summary>
    /// The virtualized Changes list: one section-header row per non-empty side
    /// (unstaged first), interleaved with that side's file rows. Headers ride in the
    /// list so the tab keeps its single scrolling surface, and the virtualizing
    /// ListBox realizes only viewport rows — the list can hold thousands of
    /// untracked files, and realizing them all was the bar's biggest RAM cost.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<object> _changeRows = new();

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
        OnPropertyChanged(nameof(IsChangesTabLoading));
        CommitCommand.NotifyCanExecuteChanged();
        GenerateCommitMessageCommand.NotifyCanExecuteChanged();
        DiscardAllCommand.NotifyCanExecuteChanged(); // CanDiscardAll reads both counts
    }

    private Task LoadChangeGroupsAsync()
    {
        var repo = Repo;
        if (repo is null)
        {
            StagedFiles.Clear();
            UnstagedFiles.Clear();
            RebuildChangeRows();
            IsLoadingGroups = false;
            RaiseChangeGroupsDerived();
            return Task.CompletedTask;
        }

        return RunBusyAsync(
            value => IsLoadingGroups = value,
            async () =>
            {
                var groups = await FetchIfCurrentAsync(repo, r => Shell.GitStatusService.GetChangeGroupsAsync(r));
                if (groups is null) return;
                ReplaceItems(StagedFiles, groups.Staged);
                ReplaceItems(UnstagedFiles, groups.Unstaged);
                RebuildChangeRows();
            },
            () => RaiseChangeGroupsDerived(),
            () => $"Could not load the staged changes of {repo.Name}");
    }

    /// <summary>
    /// Rebuilds <see cref="ChangeRows"/> from the current groups: a header row opens
    /// each non-empty side and tags every file with its side for the row template's
    /// +/− button. Mutates the collection in place (mirrors <see cref="ReplaceItems{T}"/>)
    /// so the list keeps the scroll-position behavior of the old per-section lists.
    /// </summary>
    private void RebuildChangeRows()
    {
        foreach (var file in StagedFiles) file.IsStaged = true;
        foreach (var file in UnstagedFiles) file.IsStaged = false;

        var rows = new List<object>(
            (UnstagedFiles.Count > 0 ? 1 : 0) + UnstagedFiles.Count
            + (StagedFiles.Count > 0 ? 1 : 0) + StagedFiles.Count);
        if (UnstagedFiles.Count > 0)
        {
            rows.Add(new ChangeSectionRow("Unstaged Changes", UnstagedFiles.Count, IsUnstaged: true));
            rows.AddRange(UnstagedFiles);
        }
        if (StagedFiles.Count > 0)
        {
            rows.Add(new ChangeSectionRow("Staged Changes", StagedFiles.Count, IsUnstaged: false));
            rows.AddRange(StagedFiles);
        }
        ReplaceItems(ChangeRows, rows);
    }

    // --- Staging ---
    // The four stage/unstage commands share one IsStaging busy flag (passed to
    // RunGitActionAsync as setBusy) — the buttons disable through their commands'
    // CanExecute while an index operation runs, so repeated clicks can't overlap
    // concurrent `git add`/`git reset` runs. StageFile (+) and UnstageFile (−) are
    // the per-row buttons; Stage All / Unstage All are the section headers' text buttons.

    /// <summary>True while a stage/unstage (single file or all) is running.</summary>
    [ObservableProperty]
    private bool _isStaging;

    partial void OnIsStagingChanged(bool value)
    {
        StageFileCommand.NotifyCanExecuteChanged();
        UnstageFileCommand.NotifyCanExecuteChanged();
        StageAllCommand.NotifyCanExecuteChanged();
        UnstageAllCommand.NotifyCanExecuteChanged();
        DiscardSectionCommand.NotifyCanExecuteChanged();
    }

    private bool CanStageFile(GitChangedFile? file) => !IsStaging;

    private bool CanStageAll() => !IsStaging;

    /// <summary>Stages one file (+ button on an unstaged row).</summary>
    [RelayCommand(CanExecute = nameof(CanStageFile))]
    private Task StageFileAsync(GitChangedFile? file)
    {
        var repo = Repo;
        if (repo is null || file is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            busy => IsStaging = busy,
            r => Shell.GitStatusService.StageAsync(r, file.Path),
            successText: null,
            errorText: () => $"Could not stage {file.Path}",
            onSuccess: () => _ = LoadTabAsync());
    }

    /// <summary>Unstages one file (− button on a staged row); the working tree keeps the change.</summary>
    [RelayCommand(CanExecute = nameof(CanStageFile))]
    private Task UnstageFileAsync(GitChangedFile? file)
    {
        var repo = Repo;
        if (repo is null || file is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            busy => IsStaging = busy,
            r => Shell.GitStatusService.UnstageAsync(r, file.Path),
            successText: null,
            errorText: () => $"Could not unstage {file.Path}",
            onSuccess: () => _ = LoadTabAsync());
    }

    /// <summary>
    /// Discards one row's change (↩ button). Unstaged rows revert the file's working
    /// tree to the index — untracked files are deleted; staged rows also unstage, so
    /// the file returns to HEAD. Shares the staging busy flag: no index write may
    /// overlap another. There is no undo.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanStageFile))]
    private Task DiscardFileAsync(GitChangedFile? file)
    {
        var repo = Repo;
        if (repo is null || file is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            busy => IsStaging = busy,
            r => Shell.GitStatusService.DiscardFileAsync(r, file.Path, file.IsStaged),
            successText: null,
            errorText: () => $"Could not discard {file.Path}",
            onSuccess: () => _ = LoadTabAsync());
    }

    /// <summary>Stages everything, untracked files and deletions included (Stage All).</summary>
    [RelayCommand(CanExecute = nameof(CanStageAll))]
    private Task StageAllAsync()
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            busy => IsStaging = busy,
            r => Shell.GitStatusService.StageAllAsync(r),
            successText: null,
            errorText: () => $"Could not stage the changes of {repo.Name}",
            onSuccess: () => _ = LoadTabAsync());
    }

    /// <summary>Unstages everything — index back to HEAD, working tree untouched (Unstage All).</summary>
    [RelayCommand(CanExecute = nameof(CanStageAll))]
    private Task UnstageAllAsync()
    {
        var repo = Repo;
        if (repo is null) return Task.CompletedTask;

        return RunGitActionAsync(
            repo,
            busy => IsStaging = busy,
            r => Shell.GitStatusService.UnstageAllAsync(r),
            successText: null,
            errorText: () => $"Could not unstage the changes of {repo.Name}",
            onSuccess: () => _ = LoadTabAsync());
    }

    /// <summary>
    /// Discards one whole section (the header's Discard button). Unstaged: the files'
    /// working trees revert to the index — untracked files are deleted, a partially
    /// staged file keeps its staged edits. Staged: the files return to HEAD — a
    /// staged addition is deleted. Neither has an undo; the rows only render for
    /// non-empty sections. Shares the staging busy flag.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanStageAll))]
    private Task DiscardSectionAsync(ChangeSectionRow? section)
    {
        var repo = Repo;
        if (repo is null || section is null) return Task.CompletedTask;

        var paths = (section.IsUnstaged ? UnstagedFiles : StagedFiles)
            .Select(f => f.Path)
            .ToArray();

        return RunGitActionAsync(
            repo,
            busy => IsStaging = busy,
            r => section.IsUnstaged
                ? Shell.GitStatusService.DiscardUnstagedAsync(r, paths)
                : Shell.GitStatusService.DiscardStagedAsync(r, paths),
            successText: null,
            errorText: () => $"Could not discard the changes of {repo.Name}",
            onSuccess: () => _ = LoadTabAsync());
    }

    // --- Commit ---

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
        var repo = Repo;
        var message = CommitMessage.Trim();
        if (repo is null) return;

        IsCommitting = true;
        try
        {
            if (message.Length == 0)
            {
                var token = _messageGenerator.Begin();
                string? generated;
                try
                {
                    generated = await TryGenerateCommitMessageAsync(token, repo);
                }
                catch (OperationCanceledException)
                {
                    // Repo switch or app shutdown mid-generation: abort quietly, the
                    // opencode child is already dead and repo's box must stay clean.
                    return;
                }
                finally
                {
                    _messageGenerator.End();
                }

                if (!IsSameRepo(Shell.SelectedRepo, repo)) return; // repo switched while generating

                if (generated is null)
                {
                    Shell.Notifications.Show("Could not generate a commit message", NotificationKind.Error);
                    return;
                }

                CommitMessage = generated;
                message = generated;
            }

            var hash = await Shell.GitStatusService.CommitAsync(repo, message);
            if (hash is null)
            {
                Shell.Notifications.Show("Commit failed — nothing staged, or git rejected it", NotificationKind.Error);
                return;
            }

            CommitMessage = string.Empty;
            Shell.Notifications.Show(
                hash.Length > 0 ? $"Committed {hash} to {repo.Name}" : $"Committed to {repo.Name}",
                NotificationKind.Success);
            _ = LoadTabAsync();
        }
        finally
        {
            IsCommitting = false;
        }
    }

    // --- History (the repo's recent commits, shown by the rail's History row) ---

    /// <summary>Clicking a commit's hash copies the full SHA-1 to the clipboard;
    /// the row keeps showing the seven-char display form.</summary>
    [RelayCommand]
    private async Task CopyCommitHashAsync(GitCommitInfo? commit)
    {
        if (commit is null || string.IsNullOrEmpty(commit.Hash)) return;

        await Shell.Clipboard.CopyTextAsync(commit.Hash);
        Shell.Notifications.Show($"Copied {commit.ShortHash} to clipboard", NotificationKind.Success);
    }

    /// <summary>
    /// Opens the History drawer on a clicked commit: subject, actions (checkout /
    /// revert / copy SHA), the per-file change list and the web jump. The drawer
    /// receives this bar's selected repo together with the clicked row's commit.
    /// </summary>
    [RelayCommand]
    private void OpenCommitDetail(GitCommitInfo? commit)
    {
        var repo = Repo;
        if (commit is null || repo is null) return;
        Shell.Drawers.Open(ToolComponentMapper.CommitHistoryKey, new CommitHistoryContext(repo, commit));
    }

    /// <summary>The selected repo's recent commits, newest first.</summary>
    [ObservableProperty]
    private ObservableCollection<GitCommitInfo> _gitCommits = new();

    /// <summary>True while the commit list is loading; gates the empty state.</summary>
    [ObservableProperty]
    private bool _isLoadingCommits;

    public bool ShowCommitsEmpty => !IsLoadingCommits && GitCommits.Count == 0;

    partial void OnIsLoadingCommitsChanged(bool value) => OnPropertyChanged(nameof(ShowCommitsEmpty));

    /// <summary>
    /// Whether the tab shows the full-width History view instead of the commit
    /// workspace (the rail's History row opens it, the back link closes it). Every
    /// open re-fetches the recent commits — the list may predate staging, commits
    /// or even a repo switch (which clears it), and <c>git log</c> is cheap next to
    /// a stale view.
    /// </summary>
    [ObservableProperty]
    private bool _showHistoryView;

    partial void OnShowHistoryViewChanged(bool value)
    {
        if (value)
        {
            _ = LoadRecentCommitsAsync();
        }
    }

    /// <summary>Rail History row / back link: opens and closes the History view.</summary>
    [RelayCommand]
    private Task ToggleHistoryAsync()
    {
        ShowHistoryView = !ShowHistoryView;
        return Task.CompletedTask;
    }

    /// <summary>Loads the recent commit list for the History section.</summary>
    private Task LoadRecentCommitsAsync()
    {
        var repo = Repo;
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
                var commits = await FetchIfCurrentAsync(repo, r => Shell.GitStatusService.GetRecentCommitsAsync(r));
                if (commits is null) return;
                ReplaceItems(GitCommits, commits);
            },
            () => OnPropertyChanged(nameof(ShowCommitsEmpty)),
            () => $"Could not load the recent commits of {repo.Name}");
    }

    // --- Commit-message wand (the mechanics live in CommitMessageGenerator) ---

    /// <summary>True while opencode writes the message; disables the wand button.</summary>
    [ObservableProperty]
    private bool _isGeneratingMessage;

    private bool CanGenerateCommitMessage() => Shell.HasOpenCode && HasSelectedRepo
        && StagedFiles.Count > 0 && !IsGeneratingMessage && !IsCommitting;

    partial void OnIsGeneratingMessageChanged(bool value)
    {
        GenerateCommitMessageCommand.NotifyCanExecuteChanged();
        CommitCommand.NotifyCanExecuteChanged(); // CanCommit defers to the wand when the box is empty
    }

    /// <summary>Cancels any in-flight message generation (repo switch, app shutdown).
    /// Safe to call anytime.</summary>
    public void CancelMessageGeneration() => _messageGenerator.Cancel();

    /// <summary>
    /// The wand button: asks opencode to write a commit message from the staged diff
    /// (plus the repo's recent subjects for tone) and drops it into the message box.
    /// Needs the OpenCode integration enabled and the CLI resolvable.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanGenerateCommitMessage))]
    private async Task GenerateCommitMessageAsync()
    {
        var repo = Repo;
        if (repo?.FolderPath is null) return;

        var token = _messageGenerator.Begin();
        IsGeneratingMessage = true;
        try
        {
            var message = await TryGenerateCommitMessageAsync(token, repo);
            if (!IsSameRepo(Shell.SelectedRepo, repo)) return; // repo switched while generating
            if (message is null)
            {
                Shell.Notifications.Show(
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
            _messageGenerator.End();
            IsGeneratingMessage = false;
        }
    }

    /// <summary>
    /// Runs the wand's core for the snapshotted repo: the staged paths are snapshot
    /// BEFORE the first await — a repo switch mid-run reloads the live collections,
    /// and the prompt must not mix the old diff with the new repo's context. The
    /// tone subjects are read fresh from git against the snapshotted repo (the tab
    /// keeps no history list); a failed log just drops the tone context.
    /// </summary>
    private async Task<string?> TryGenerateCommitMessageAsync(CancellationToken cancellationToken, Repo repo)
    {
        var stagedPaths = StagedFiles.Select(f => f.Path).ToArray();

        string[] recentSubjects = [];
        try
        {
            var commits = await Shell.GitStatusService.GetRecentCommitsAsync(repo, cancellationToken);
            recentSubjects = commits.Take(5).Select(c => c.Subject).ToArray();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Bottom bar recent-subject read failed for {Path}", repo.FolderPath);
        }

        return await _messageGenerator.TryGenerateAsync(
            cancellationToken,
            repo,
            stagedPaths,
            recentSubjects,
            r => Shell.GitStatusService.GetStagedPatchAsync(r),
            Shell.ReposSettings.OpenCodeExecutable,
            Shell.ResolveWandModel());
    }
}
