using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// The open payload for the History drawer component: the repo whose commit is shown
/// plus the History row the user clicked.
/// </summary>
public sealed record CommitHistoryContext(Repo Repo, GitCommitInfo Commit);

/// <summary>
/// One History drawer: the clicked commit's subject/hash/author, its actions
/// (checkout, revert, copy SHA), the per-file changed list with expandable patches,
/// and the "View Full Diff" jump to the GitHub/Azure DevOps web page.
/// </summary>
public partial class CommitHistoryViewModel : ObservableObject, IToolDrawerContextReceiver<CommitHistoryContext>
{
    private readonly IGitStatusService _gitStatusService;
    private readonly IProcessLauncher _processLauncher;
    private readonly IToolDrawerService _toolDrawer;
    private readonly INotificationService _notificationService;
    private readonly IClipboardService _clipboardService;

    private Repo? _repo;

    public CommitHistoryViewModel(
        IGitStatusService gitStatusService,
        IProcessLauncher processLauncher,
        IToolDrawerService toolDrawer,
        INotificationService notificationService,
        IClipboardService clipboardService)
    {
        _gitStatusService = gitStatusService;
        _processLauncher = processLauncher;
        _toolDrawer = toolDrawer;
        _notificationService = notificationService;
        _clipboardService = clipboardService;
    }

    /// <summary>The clicked commit (subject, hash, author, time).</summary>
    [ObservableProperty]
    private GitCommitInfo? _commit;

    partial void OnCommitChanged(GitCommitInfo? value)
    {
        OnPropertyChanged(nameof(CommitSubject));
        OnPropertyChanged(nameof(CommitShortHash));
        OnPropertyChanged(nameof(CommitAuthor));
        OnPropertyChanged(nameof(CommitRelativeTime));
    }

    /// <summary>Null-safe header mirrors of <see cref="Commit"/> — the drawer's bindings
    /// attach before the open context lands, and a null intermediate path logs a binding
    /// error under a debugger on every open.</summary>
    public string? CommitSubject => Commit?.Subject;

    public string? CommitShortHash => Commit?.ShortHash;

    public string? CommitAuthor => Commit?.Author;

    public string? CommitRelativeTime => Commit?.RelativeTime;

    /// <summary>True while the commit's file list is being loaded.</summary>
    [ObservableProperty]
    private bool _isLoading;

    partial void OnIsLoadingChanged(bool value)
    {
        OnPropertyChanged(nameof(ShowNoFilesNote));
    }

    /// <summary>True while a checkout/revert is running; disables both actions.</summary>
    [ObservableProperty]
    private bool _isBusy;

    /// <summary>True after the first Checkout click; the next click confirms it
    /// (detached HEAD is destructive enough to ask twice).</summary>
    [ObservableProperty]
    private bool _isCheckoutArmed;

    partial void OnIsCheckoutArmedChanged(bool value)
    {
        OnPropertyChanged(nameof(CheckoutLabel));
        OnPropertyChanged(nameof(CheckoutTooltip));
    }

    /// <summary>True after the first Revert click; the next click confirms it
    /// (revert writes a new commit to the branch).</summary>
    [ObservableProperty]
    private bool _isRevertArmed;

    partial void OnIsRevertArmedChanged(bool value)
    {
        OnPropertyChanged(nameof(RevertLabel));
        OnPropertyChanged(nameof(RevertTooltip));
    }

    /// <summary>The checkout button's two-state label (arm → confirm).</summary>
    public string CheckoutLabel => IsCheckoutArmed ? "Confirm checkout" : "Checkout";

    /// <summary>The revert button's two-state label (arm → confirm).</summary>
    public string RevertLabel => IsRevertArmed ? "Confirm revert" : "Revert";

    public string CheckoutTooltip => IsCheckoutArmed
        ? $"Click again to check out {CommitShortHash} (detached HEAD)"
        : "Check out this commit (detached HEAD)";

    public string RevertTooltip => IsRevertArmed
        ? $"Click again to revert {CommitShortHash} (a new commit undoes it)"
        : "Revert this commit (a new commit undoes it)";

    private void DisarmActions()
    {
        IsCheckoutArmed = false;
        IsRevertArmed = false;
    }

    /// <summary>The commit's changed files with per-file counts and expansion state.</summary>
    [ObservableProperty]
    private ObservableCollection<CommitFileRowViewModel> _files = new();

    /// <summary>Whether a GitHub/Azure DevOps web page can back the "View Full Diff" action.</summary>
    [ObservableProperty]
    private bool _hasWebUrl;

    private string? _webUrl;

    /// <summary>"4 files changed" summary line; empty while loading or with no files.</summary>
    public string FilesChangedText => Files.Count == 0
        ? string.Empty
        : $"{Files.Count} file{(Files.Count == 1 ? string.Empty : "s")} changed";

    /// <summary>Whether the details finished loading without reporting any changed file
    /// (e.g. a merge commit, which numstat lists as empty).</summary>
    public bool ShowNoFilesNote => !IsLoading && !LoadFailed && Files.Count == 0;

    /// <summary>Whether the commit's details failed to load (git error) — shown as its
    /// own note instead of the misleading "no file changes" empty state.</summary>
    [ObservableProperty]
    private bool _loadFailed;

    partial void OnLoadFailedChanged(bool value)
    {
        OnPropertyChanged(nameof(ShowNoFilesNote));
    }

    /// <summary>Total "+N" across the commit; null while nothing reports counts.</summary>
    public int? TotalAdditions => _totalAdditions;

    /// <summary>Total "−N" across the commit; null while nothing reports counts.</summary>
    public int? TotalDeletions => _totalDeletions;

    private int? _totalAdditions;
    private int? _totalDeletions;

    private void RaiseTotals()
    {
        OnPropertyChanged(nameof(TotalAdditions));
        OnPropertyChanged(nameof(TotalDeletions));
        OnPropertyChanged(nameof(FilesChangedText));
        OnPropertyChanged(nameof(ShowNoFilesNote));
    }

    /// <inheritdoc/>
    public Task OnDrawerContextAsync(CommitHistoryContext? context)
    {
        if (context is null)
        {
            return Task.CompletedTask;
        }

        _repo = context.Repo;
        Commit = context.Commit;
        Files.Clear();
        IsLoading = false;
        IsBusy = false;
        LoadFailed = false;
        DisarmActions();
        HasWebUrl = false;
        _webUrl = null;

        ComputeWebUrl();
        _ = LoadDetailsAsync();
        return Task.CompletedTask;
    }

    /// <summary>
    /// The "View Full Diff" target: the GitHub commit page when the repo has a GitHub
    /// remote, else the Azure DevOps commit page. Hidden when neither exists.
    /// </summary>
    private void ComputeWebUrl()
    {
        if (_repo is null || Commit is null)
        {
            return;
        }

        if (_repo.GitHubRepoUrl is { } github)
        {
            _webUrl = $"{github.TrimEnd('/')}/commit/{Commit.Hash}";
        }
        else if (_repo.AzureDevOpsRepoUrl is { } azure)
        {
            _webUrl = $"{azure.TrimEnd('/')}/commit/{Commit.Hash}";
        }

        HasWebUrl = _webUrl is not null;
    }

    private async Task LoadDetailsAsync()
    {
        var repo = _repo;
        if (repo?.FolderPath is null || Commit is null) return;

        IsLoading = true;
        try
        {
            var details = await _gitStatusService.GetCommitDetailsAsync(repo, Commit.Hash);
            Files.Clear();
            foreach (var file in details.Files)
            {
                Files.Add(new CommitFileRowViewModel(_gitStatusService, repo, Commit.Hash, file));
            }

            _totalAdditions = details.Additions;
            _totalDeletions = details.Deletions;
            RaiseTotals();
        }
        catch (Exception ex)
        {
            // The call site discards this task, so the failure would otherwise render
            // as the "no file changes" empty state — surface it as its own note instead.
            Log.Logger.Error(ex, "Commit details failed for {Hash} in {FolderPath}", Commit.Hash, repo.FolderPath);
            Files.Clear();
            _totalAdditions = null;
            _totalDeletions = null;
            RaiseTotals();
            LoadFailed = true;
        }
        finally
        {
            IsLoading = false;
        }
    }

    /// <summary>Drawer back link: mirrors the header's X close.</summary>
    [RelayCommand]
    private void Close()
    {
        _toolDrawer.Close();
    }

    /// <summary>
    /// The two git actions' shared mechanics: run under the drawer's busy flag and toast
    /// the given success/error message. The null repo/commit and already-busy guards stay
    /// with the commands. A thrown exception is treated as a failed outcome (logged and
    /// toasted) — the relay commands would otherwise stash it unobserved. Both confirm
    /// states clear when the action settles: an armed click either executes or disarms.
    /// </summary>
    private async Task RunGitActionAsync(Func<Task<bool>> action, string successText, string errorText)
    {
        IsBusy = true;
        try
        {
            if (await action())
            {
                _notificationService.Show(successText, NotificationKind.Success);
            }
            else
            {
                _notificationService.Show(errorText, NotificationKind.Error);
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "History drawer git action failed for {Hash}", Commit?.Hash);
            _notificationService.Show(errorText, NotificationKind.Error);
        }
        finally
        {
            DisarmActions();
            IsBusy = false;
        }
    }

    /// <summary>Checks out the commit (detached HEAD at the hash): the first click arms,
    /// the second confirms.</summary>
    [RelayCommand]
    private Task CheckoutAsync()
    {
        var repo = _repo;
        var commit = Commit;
        if (repo is null || commit is null || IsBusy) return Task.CompletedTask;

        if (!IsCheckoutArmed)
        {
            IsCheckoutArmed = true;
            IsRevertArmed = false;
            return Task.CompletedTask;
        }

        IsCheckoutArmed = false;
        return RunGitActionAsync(
            () => _gitStatusService.CheckoutAsync(repo, commit.Hash),
            $"Checked out {commit.ShortHash}",
            $"Checkout of {commit.ShortHash} failed");
    }

    /// <summary>Reverts the commit (<c>git revert --no-edit</c>: a new undo commit):
    /// the first click arms, the second confirms.</summary>
    [RelayCommand]
    private Task RevertAsync()
    {
        var repo = _repo;
        var commit = Commit;
        if (repo is null || commit is null || IsBusy) return Task.CompletedTask;

        if (!IsRevertArmed)
        {
            IsRevertArmed = true;
            IsCheckoutArmed = false;
            return Task.CompletedTask;
        }

        IsRevertArmed = false;
        return RunGitActionAsync(
            () => _gitStatusService.RevertCommitAsync(repo, commit.Hash),
            $"Reverted {commit.ShortHash}",
            $"Revert of {commit.ShortHash} failed");
    }

    /// <summary>Copies the full SHA (same flow as the History row's hash link).</summary>
    [RelayCommand]
    private async Task CopyShaAsync()
    {
        if (Commit is null || string.IsNullOrEmpty(Commit.Hash)) return;

        await _clipboardService.CopyTextAsync(Commit.Hash);
        _notificationService.Show($"Copied {Commit.ShortHash} to clipboard", NotificationKind.Success);
    }

    /// <summary>Opens the commit's GitHub/Azure DevOps page (the full diff) in the browser.</summary>
    [RelayCommand]
    private void OpenFullDiff()
    {
        if (_webUrl is { } url)
        {
            _processLauncher.StartProcess(url);
        }
    }
}

/// <summary>
/// One file row of the History drawer: the changed file with its +/− counts and an
/// expandable patch. The patch loads from <c>git show &lt;hash&gt; -- &lt;path&gt;</c>
/// the first time the row is expanded.
/// </summary>
public partial class CommitFileRowViewModel : ObservableObject
{
    private readonly IGitStatusService _gitStatusService;
    private readonly Repo _repo;
    private readonly string _hash;

    public CommitFileRowViewModel(IGitStatusService gitStatusService, Repo repo, string hash, GitChangedFile file)
    {
        _gitStatusService = gitStatusService;
        _repo = repo;
        _hash = hash;
        File = file;
    }

    public GitChangedFile File { get; }

    public string Path => File.Path;

    public string? AddedText => File.AddedText;

    public string? RemovedText => File.RemovedText;

    /// <summary>Whether the per-file patch is expanded.</summary>
    [ObservableProperty]
    private bool _isExpanded;

    /// <summary>The loaded patch text; null until the first expansion completes.</summary>
    [ObservableProperty]
    private string? _patch;

    /// <summary>True while the patch is being fetched.</summary>
    [ObservableProperty]
    private bool _isLoadingPatch;

    [RelayCommand]
    private async Task ToggleAsync()
    {
        IsExpanded = !IsExpanded;
        if (IsExpanded && Patch is null && !IsLoadingPatch)
        {
            IsLoadingPatch = true;
            try
            {
                Patch = await _gitStatusService.GetCommitFilePatchAsync(_repo, _hash, File.Path);
            }
            finally
            {
                IsLoadingPatch = false;
            }
        }
    }
}
