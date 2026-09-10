using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// The open payload for the new-branch drawer: the repo to branch in, the configured
/// branch-name prefix (pre-typed into the name field so the user only completes it)
/// and the completion hook the Changes tab passes to re-sync its branch dropdown once
/// a branch landed.
/// </summary>
public sealed record NewBranchContext(Repo Repo, string? BranchPrefix, Action? OnCreated = null);

/// <summary>
/// One new-branch drawer, opened from the Changes tab's branch dropdown ("New branch…"
/// entry): the name field (pre-typed with the settings' branch-name prefix), the base
/// branch picker (defaults to the repo's current branch), the check-out-after-create
/// toggle and the Create action. The git work runs through
/// <see cref="IGitStatusService.CreateBranchAsync"/>; failures (duplicate or invalid
/// name) surface as an inline note under the field.
/// </summary>
public partial class NewBranchViewModel : ObservableObject, IToolDrawerContextReceiver<NewBranchContext>
{
    private readonly IGitStatusService _gitStatusService;
    private readonly IToolDrawerService _toolDrawer;
    private readonly INotificationService _notificationService;

    /// <summary>The drawer's repo, snapshotted at open; null until the context lands.</summary>
    private Repo? _repo;

    /// <summary>The Changes tab's post-create hook (branch dropdown re-sync).</summary>
    private Action? _onCreated;

    public NewBranchViewModel(
        IGitStatusService gitStatusService,
        IToolDrawerService toolDrawer,
        INotificationService notificationService)
    {
        _gitStatusService = gitStatusService;
        _toolDrawer = toolDrawer;
        _notificationService = notificationService;
    }

    /// <summary>The new branch's name. Pre-typed with the settings' prefix closed by its
    /// path separator, so the caret starts right where the user's own name part goes.</summary>
    [ObservableProperty]
    private string? _branchName;

    /// <summary>The branch the new one starts from (the drawer's local branch list).</summary>
    [ObservableProperty]
    private ObservableCollection<string> _baseBranches = new();

    [ObservableProperty]
    private string? _selectedBaseBranch;

    /// <summary>Whether Create also checks the branch out (git checkout -b vs git branch).</summary>
    [ObservableProperty]
    private bool _checkoutAfterCreate = true;

    /// <summary>True while the branch is being created; disables the Create button.</summary>
    [ObservableProperty]
    private bool _isCreating;

    partial void OnIsCreatingChanged(bool value) => CreateCommand.NotifyCanExecuteChanged();

    /// <summary>Inline validation / git-failure note under the name field; null hides it.</summary>
    [ObservableProperty]
    private string? _errorText;

    public bool HasError => !string.IsNullOrEmpty(ErrorText);

    partial void OnErrorTextChanged(string? value) => OnPropertyChanged(nameof(HasError));

    /// <inheritdoc/>
    public Task OnDrawerContextAsync(NewBranchContext? context)
    {
        if (context is null)
        {
            return Task.CompletedTask;
        }

        _repo = context.Repo;
        _onCreated = context.OnCreated;
        ErrorText = null;
        IsCreating = false;
        CheckoutAfterCreate = true;

        // Pre-type the configured prefix, closed with its separator so the first
        // keystroke continues after it; a prefix already ending in one is kept
        // verbatim, and an empty setting leaves the field empty.
        var prefix = context.BranchPrefix?.Trim();
        BranchName = string.IsNullOrEmpty(prefix)
            ? string.Empty
            : prefix.EndsWith('/') ? prefix : prefix + "/";

        BaseBranches.Clear();
        SelectedBaseBranch = null;
        _ = LoadBaseBranchesAsync();

        // The Button evaluated CanExecute at bind time — before this context landed —
        // and Avalonia never requeries commands: without this notify the Create button
        // would stay disabled for the whole drawer open.
        CreateCommand.NotifyCanExecuteChanged();
        return Task.CompletedTask;
    }

    /// <summary>
    /// Loads the base picker's options (the repo's local branches) and defaults the
    /// selection to the repo's current branch. A stale load (the drawer reopened on
    /// another repo before this one settled) is dropped.
    /// </summary>
    private async Task LoadBaseBranchesAsync()
    {
        var repo = _repo;
        if (repo?.FolderPath is null) return;

        try
        {
            var branches = await _gitStatusService.GetBranchesAsync(repo);
            if (!ReferenceEquals(_repo, repo)) return;

            // The base picker branches from local heads only — remote-tracking refs
            // stay in the Changes tab's dropdown where they map to a checkout action.
            BaseBranches.Clear();
            foreach (var branch in branches.Where(b => b.IsLocal).Select(b => b.Name))
            {
                BaseBranches.Add(branch);
            }

            SelectedBaseBranch = repo.GitBranchName is { } current && BaseBranches.Contains(current)
                ? current
                : BaseBranches.FirstOrDefault();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "New-branch drawer base list load failed for {Path}", repo.FolderPath);
        }
    }

    private bool CanCreate() => !IsCreating && _repo is not null;

    /// <summary>
    /// Creates the branch: validates the name locally (the common mistakes — an empty
    /// field, git's illegal characters, a duplicate of an existing local branch), then
    /// runs the creation. A git rejection (e.g. a remote-only name, a trailing ".lock")
    /// lands in the same inline note from its captured stderr. Success toasts, closes
    /// the drawer and fires the Changes tab's re-sync hook.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanCreate))]
    private async Task CreateAsync()
    {
        var repo = _repo;
        if (repo is null) return;

        // A lone prefix (the field pre-types "users/x/") is not a name; the trailing
        // separator the prefill added may be trimmed back off.
        var name = BranchName?.Trim().TrimEnd('/');
        if (string.IsNullOrEmpty(name))
        {
            ErrorText = "Enter a branch name.";
            return;
        }
        if (BranchNameError(name) is { } invalid)
        {
            ErrorText = invalid;
            return;
        }
        if (BaseBranches.Any(b => string.Equals(b, name, StringComparison.OrdinalIgnoreCase)))
        {
            ErrorText = $"A branch named \"{name}\" already exists.";
            return;
        }

        ErrorText = null;
        IsCreating = true;
        try
        {
            var result = await _gitStatusService.CreateBranchAsync(
                repo,
                name,
                SelectedBaseBranch,
                CheckoutAfterCreate);
            if (!result.Success)
            {
                ErrorText = result.Error is { } detail
                    ? $"git rejected it: {detail}"
                    : "git could not create the branch.";
                return;
            }

            _notificationService.Show(
                CheckoutAfterCreate ? $"Created and checked out {name}" : $"Created {name}",
                NotificationKind.Success);
            _toolDrawer.Close();
            _onCreated?.Invoke();
        }
        finally
        {
            IsCreating = false;
        }
    }

    /// <summary>Drawer back link / footer Cancel: mirrors the header's X close.</summary>
    [RelayCommand]
    private void Cancel() => _toolDrawer.Close();

    /// <summary>
    /// The branch-name rules worth catching before git does (git's own rejection then
    /// only covers the exotic remainder): the separator characters, <c>..</c>, a
    /// leading dash (reads as a git flag) and a trailing dot or slash. Returns null
    /// when the name is acceptable.
    /// </summary>
    private static string? BranchNameError(string name)
    {
        if (name.Contains(' ') || name.Contains('~') || name.Contains('^') || name.Contains(':')
            || name.Contains('?') || name.Contains('*') || name.Contains('[') || name.Contains('\\'))
        {
            return "Branch names cannot contain spaces or any of ~ ^ : ? * [ \\";
        }
        if (name.Contains(".."))
        {
            return "Branch names cannot contain \"..\".";
        }
        if (name.StartsWith('-') || name.StartsWith('+'))
        {
            return "Branch names cannot start with '-' or '+'.";
        }
        if (name.EndsWith('.'))
        {
            return "Branch names cannot end with a dot.";
        }
        if (name.StartsWith('/') || name.EndsWith('/') || name.Contains("//"))
        {
            return "Branch names cannot start, end with, or repeat '/'.";
        }
        return null;
    }
}
