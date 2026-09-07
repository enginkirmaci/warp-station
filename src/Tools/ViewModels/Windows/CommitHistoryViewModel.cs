using System.Collections.ObjectModel;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Input;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
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
public partial class CommitHistoryViewModel : ObservableObject, IToolDrawerContextReceiver
{
    private readonly IGitStatusService _gitStatusService;
    private readonly IProcessLauncher _processLauncher;
    private readonly IToolDrawerService _toolDrawer;
    private readonly INotificationService _notificationService;

    private Repo? _repo;

    public CommitHistoryViewModel(
        IGitStatusService gitStatusService,
        IProcessLauncher processLauncher,
        IToolDrawerService toolDrawer,
        INotificationService notificationService)
    {
        _gitStatusService = gitStatusService;
        _processLauncher = processLauncher;
        _toolDrawer = toolDrawer;
        _notificationService = notificationService;
    }

    /// <summary>The clicked commit (subject, hash, author, time).</summary>
    [ObservableProperty]
    private GitCommitInfo? _commit;

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
    public bool ShowNoFilesNote => !IsLoading && Files.Count == 0;

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
    public void OnDrawerContext(object context)
    {
        if (context is not CommitHistoryContext payload)
        {
            return;
        }

        _repo = payload.Repo;
        Commit = payload.Commit;
        Files.Clear();
        IsLoading = false;
        IsBusy = false;
        HasWebUrl = false;
        _webUrl = null;

        ComputeWebUrl();
        _ = LoadDetailsAsync();
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

    /// <summary>Checks out the commit (detached HEAD at the hash) and refreshes status.</summary>
    [RelayCommand]
    private async Task CheckoutAsync()
    {
        var repo = _repo;
        if (repo is null || Commit is null || IsBusy) return;

        IsBusy = true;
        try
        {
            if (await _gitStatusService.CheckoutAsync(repo, Commit.Hash))
            {
                _notificationService.Show($"Checked out {Commit.ShortHash}", NotificationKind.Success);
            }
            else
            {
                _notificationService.Show($"Checkout of {Commit.ShortHash} failed", NotificationKind.Error);
            }
        }
        finally
        {
            IsBusy = false;
        }
    }

    /// <summary>Reverts the commit (<c>git revert --no-edit</c>: a new undo commit).</summary>
    [RelayCommand]
    private async Task RevertAsync()
    {
        var repo = _repo;
        if (repo is null || Commit is null || IsBusy) return;

        IsBusy = true;
        try
        {
            if (await _gitStatusService.RevertCommitAsync(repo, Commit.Hash))
            {
                _notificationService.Show($"Reverted {Commit.ShortHash}", NotificationKind.Success);
            }
            else
            {
                _notificationService.Show($"Revert of {Commit.ShortHash} failed", NotificationKind.Error);
            }
        }
        finally
        {
            IsBusy = false;
        }
    }

    /// <summary>Copies the full SHA (same flow as the History row's hash link).</summary>
    [RelayCommand]
    private async Task CopyShaAsync()
    {
        if (Commit is null || string.IsNullOrEmpty(Commit.Hash)) return;

        if (Application.Current?.ApplicationLifetime
            is IClassicDesktopStyleApplicationLifetime { MainWindow: { } window })
        {
            var transfer = new DataTransfer();
            transfer.Add(DataTransferItem.CreateText(Commit.Hash));
            await window.Clipboard.SetDataAsync(transfer);
            _notificationService.Show($"Copied {Commit.ShortHash} to clipboard", NotificationKind.Success);
        }
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
