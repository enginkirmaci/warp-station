using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// The open payload for the clone-from-URL drawer: the destination folder the field
/// comes pre-typed with (derived from the repos page's scan roots — the first root's
/// parent folder, so the default clone lands next to the tracked repos) and the
/// completion hook the page passes so the cloned repository is registered — scan root
/// if needed, rescan, bottom bar opened on it.
/// </summary>
public sealed record CloneFromUrlContext(string? DefaultDestination, Func<string, Task>? OnCloned = null);

/// <summary>
/// One clone-from-URL drawer, opened from the repos page toolbar: the repository URL,
/// the destination folder name (auto-derived from the URL until the user edits it) and
/// the destination folder (pre-typed with the first scan root). The git work runs
/// through <see cref="IGitStatusService.CloneAsync"/>; failures (unknown host, missing
/// credentials, existing destination) surface as an inline note under the fields.
/// </summary>
public partial class CloneFromUrlViewModel : ObservableObject,
    IToolDrawerContextReceiver<CloneFromUrlContext>, IToolDrawerTeardown
{
    private readonly IGitStatusService _gitStatusService;
    private readonly IToolDrawerService _toolDrawer;
    private readonly INotificationService _notificationService;

    /// <summary>The page's post-clone hook (register the repo, open the bar on it).</summary>
    private Func<string, Task>? _onCloned;

    /// <summary>Cancels the in-flight clone (its git process tree dies with it); null while idle.</summary>
    private CancellationTokenSource? _cloneCts;

    public CloneFromUrlViewModel(
        IGitStatusService gitStatusService,
        IToolDrawerService toolDrawer,
        INotificationService notificationService)
    {
        _gitStatusService = gitStatusService;
        _toolDrawer = toolDrawer;
        _notificationService = notificationService;
    }

    /// <summary>The repository source URL (https, ssh or a local path — anything git clone takes).</summary>
    [ObservableProperty]
    private string? _repositoryUrl;

    /// <summary>The destination folder's name, auto-derived from the URL's last segment
    /// (git's own naming) until the user edits it by hand.</summary>
    [ObservableProperty]
    private string? _repoName;

    /// <summary>The folder the repository is cloned into; the repo itself lands one
    /// level below it. Pre-typed with the page's first scan root.</summary>
    [ObservableProperty]
    private string? _destinationFolder;

    /// <summary>True while the clone is running; disables the Clone button and spins its ring.</summary>
    [ObservableProperty]
    private bool _isCloning;

    partial void OnIsCloningChanged(bool value) => CloneCommand.NotifyCanExecuteChanged();

    /// <summary>Inline validation / git-failure note under the fields; null hides it.</summary>
    [ObservableProperty]
    private string? _errorText;

    public bool HasError => !string.IsNullOrEmpty(ErrorText);

    partial void OnErrorTextChanged(string? value) => OnPropertyChanged(nameof(HasError));

    /// <inheritdoc/>
    public Task OnDrawerContextAsync(CloneFromUrlContext? context)
    {
        if (context is null)
        {
            return Task.CompletedTask;
        }

        _onCloned = context.OnCloned;
        _cloneCts?.Dispose();
        _cloneCts = null;
        ErrorText = null;
        IsCloning = false;
        RepositoryUrl = null;
        RepoName = null;
        DestinationFolder = context.DefaultDestination;

        // The Button evaluated CanExecute at bind time — before this context landed —
        // and Avalonia never requeries commands: without this notify the Clone button
        // would stay disabled for the whole drawer open.
        CloneCommand.NotifyCanExecuteChanged();
        return Task.CompletedTask;
    }

    private bool CanClone() => !IsCloning;

    /// <summary>
    /// Clones the repository: validates the fields locally (a source URL, a single-folder
    /// name, a destination that exists and whose target slot is free), then runs the
    /// clone. A git rejection (unknown host, missing credentials, an existing target)
    /// lands in the same inline note from its captured stderr. Success toasts, closes the
    /// drawer and fires the page's registration hook with the cloned folder's path.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanClone))]
    private async Task CloneAsync()
    {
        var url = RepositoryUrl?.Trim();
        if (string.IsNullOrEmpty(url))
        {
            ErrorText = "Enter a repository URL.";
            return;
        }
        if (!url.Contains('/') && !url.Contains(':'))
        {
            ErrorText = "That doesn't look like a repository URL — e.g. https://host/owner/repo.git.";
            return;
        }

        var name = RepoName?.Trim();
        if (string.IsNullOrEmpty(name))
        {
            ErrorText = "Enter a destination folder name.";
            return;
        }
        if (name is "." or ".." || name.Contains('/') || name.Contains('\\'))
        {
            ErrorText = "The folder name must be a single name, not a path.";
            return;
        }

        var destination = DestinationFolder?.Trim();
        if (string.IsNullOrEmpty(destination))
        {
            ErrorText = "Enter a destination folder.";
            return;
        }
        if (!Directory.Exists(destination))
        {
            ErrorText = $"The folder \"{destination}\" does not exist.";
            return;
        }
        var target = Path.Combine(destination, name);
        if (File.Exists(target) || Directory.Exists(target))
        {
            ErrorText = $"\"{target}\" already exists.";
            return;
        }

        ErrorText = null;
        IsCloning = true;
        using var cts = new CancellationTokenSource();
        _cloneCts = cts;
        try
        {
            var result = await _gitStatusService.CloneAsync(url, destination, name, cts.Token);
            if (result.Cancelled)
            {
                // The Cancel button usually closed the drawer already; the note only
                // shows when the drawer is somehow still open.
                ErrorText = "Clone cancelled.";
                return;
            }

            if (!result.Success)
            {
                ErrorText = result.Error is { } detail
                    ? $"git rejected it: {detail}"
                    : "git could not clone the repository.";
                return;
            }

            _notificationService.Show($"Cloned {name}", NotificationKind.Success);
            _toolDrawer.Close();
            if (_onCloned is { } onCloned)
            {
                await onCloned(target);
            }
        }
        finally
        {
            _cloneCts = null;
            IsCloning = false;
        }
    }

    /// <summary>
    /// Drawer back link / footer Cancel: mirrors the header's X close. While a clone
    /// runs, Cancel aborts it first — the token fires and the whole git clone tree is
    /// killed (with the partial destination removed) instead of running on toward its
    /// ten minute bound.
    /// </summary>
    [RelayCommand]
    private void Cancel()
    {
        if (IsCloning)
        {
            _cloneCts?.Cancel();
        }
        _toolDrawer.Close();
    }

    /// <summary>
    /// Runs when the drawer stops showing this component by ANY close path (header X,
    /// backdrop, Escape, another tool picked) — previously only the footer Cancel
    /// cancelled the clone, so a dismissed drawer kept cloning toward its ten minute
    /// bound and wrote into the destination folder after the user walked away.
    /// </summary>
    public void OnDrawerClosed()
    {
        if (IsCloning)
        {
            _cloneCts?.Cancel();
        }
    }
}
