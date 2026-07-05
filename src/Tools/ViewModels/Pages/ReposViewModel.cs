using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Formatters;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// Binding adapter for the Repos page. Delegates scanning, caching, and the shared
/// repo state to <see cref="IRepoService"/> (singleton), process launching to
/// <see cref="IProcessLauncher"/>, and tag persistence back through the service.
/// Holds only view-specific state: the text + tag filters, the filtered projection,
/// and the transient OpenCode panel options.
/// </summary>
public partial class ReposViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly IDevToolsClient _devToolsClient;
    private readonly IDialogService _dialogService;
    private readonly IRepoService _repoService;
    private readonly IProcessLauncher _processLauncher;
    private ReposSettings _reposSettings = new();

    [ObservableProperty]
    private string _filterText = string.Empty;

    [ObservableProperty]
    private ObservableCollection<Repo> _filteredRepos = new();

    /// <summary>
    /// The checkable tag list shown in the left filter panel. Rebuilt from
    /// <see cref="IRepoService.AllTags"/> whenever the service changes, preserving
    /// existing check states by tag name so checking a tag survives a rescan.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<TagFilter> _tagFilters = new();

    // --- OpenCode panel (transient state) ---

    [ObservableProperty]
    private bool _isOpenCodePanelOpen;

    [ObservableProperty]
    private Repo? _openCodeRepo;

    [ObservableProperty]
    private int _openCodeInstanceCount = 1;

    [ObservableProperty]
    private string _openCodePrompt = string.Empty;

    /// <summary>
    /// Bound to the add-tag flyout TextBox; cleared after a tag is added.
    /// </summary>
    [ObservableProperty]
    private string _newTagText = string.Empty;

    /// <summary>
    /// Existing tags the user can quickly add from the add-tag flyout (everything
    /// currently in use, minus the auto-tag <c>platform</c> since it is auto-assigned).
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<string> _availableTags = new();

    public ReposViewModel(
        ISettingsService settingsService,
        IDevToolsClient devToolsClient,
        IDialogService dialogService,
        IRepoService repoService,
        IProcessLauncher processLauncher)
    {
        _settingsService = settingsService;
        _devToolsClient = devToolsClient;
        _dialogService = dialogService;
        _repoService = repoService;
        _processLauncher = processLauncher;

        _repoService.Changed += OnRepoChanged;
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override Task OnNavigatedFromAsync()
    {
        // Detach from the singleton so this Transient VM (rebuilt per navigation) is not
        // kept alive by the service and does not receive further state changes.
        _repoService.Changed -= OnRepoChanged;
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        _reposSettings = settings.Repos ?? new ReposSettings();
        await _repoService.EnsureLoadedAsync(_reposSettings);
        RebuildTagFilters();
        ApplyFilter();
    }

    private void OnRepoChanged(object? sender, EventArgs e)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            IsBusy = _repoService.IsBusy;
            RebuildTagFilters();
            ApplyFilter();
        });
    }

    partial void OnFilterTextChanged(string value) => ApplyFilter();

    /// <summary>
    /// Re-apply the filter when the panel closes so the OpenCode-targeted repo is not
    /// left visually selected.
    /// </summary>
    partial void OnIsOpenCodePanelOpenChanged(bool value)
    {
        if (!value)
        {
            OpenCodeRepo = null;
            OpenCodeInstanceCount = 1;
            OpenCodePrompt = string.Empty;
        }
    }

    [RelayCommand]
    private void ClearFilter() => FilterText = string.Empty;

    /// <summary>
    /// Clears every tag checkbox (does not touch the tags on the repos themselves).
    /// </summary>
    [RelayCommand]
    private void ClearTagFilters()
    {
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
        AvailableTags = new ObservableCollection<string>(
            _repoService.AllTags
                .Where(t => !string.Equals(t, Repo.PlatformTag, StringComparison.OrdinalIgnoreCase))
                .OrderBy(t => t, StringComparer.OrdinalIgnoreCase));
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

        // Favorites always float to the top, then alphabetical by name.
        FilteredRepos = new ObservableCollection<Repo>(
            result
                .OrderByDescending(r => r.IsFavorite)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase));
    }

    // --- Launch commands ---

    [RelayCommand]
    private void OpenVisualStudio(Repo? repo)
    {
        if (repo?.SolutionPath is null) return;
        _processLauncher.StartProcess(repo.SolutionPath);
    }

    [RelayCommand]
    private void OpenFolder(string? folderPath) => _processLauncher.StartProcess(folderPath);

    [RelayCommand]
    private async Task OpenWithVSCodeAsync(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = _reposSettings.VSCodeExecutable ?? "code";

        // Route through the DevTools service (named pipe). The service runs
        // non-elevated, so VS Code launches non-elevated even when Tools runs as admin.
        try
        {
            await _devToolsClient.SendProcessLaunchRequestAsync(exe, folderPath);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenWithVSCode: pipe launch failed, falling back to direct launch");
            _processLauncher.StartProcess(exe, folderPath, hidden: true);
        }
    }

    [RelayCommand]
    private void OpenWithTerminal(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = _reposSettings.TerminalExecutable ?? "wt";
        var args = TerminalArgumentFormatter.BuildArguments(exe, folderPath);
        _processLauncher.StartProcess(exe, args);
    }

    // --- OpenCode panel ---

    [RelayCommand]
    private void OpenOpenCodePanel(Repo? repo)
    {
        if (repo is null) return;
        OpenCodeRepo = repo;
        OpenCodeInstanceCount = 1;
        OpenCodePrompt = string.Empty;
        IsOpenCodePanelOpen = true;
    }

    [RelayCommand]
    private void CloseOpenCodePanel() => IsOpenCodePanelOpen = false;

    [RelayCommand]
    private async Task LaunchOpenCodeAsync()
    {
        var repo = OpenCodeRepo;
        if (repo?.FolderPath is null) return;

        var terminalExe = _reposSettings.TerminalExecutable ?? "wt";
        var openCodeExe = _reposSettings.OpenCodeExecutable ?? "opencode";
        var prompt = OpenCodePrompt?.Trim();
        var count = OpenCodeInstanceCount < 1 ? 1 : OpenCodeInstanceCount;

        // Build the inner command line: opencode "prompt" when a prompt is given,
        // otherwise plain opencode.
        var commandLine = string.IsNullOrWhiteSpace(prompt)
            ? openCodeExe
            : $"{openCodeExe} \"{EscapeForCommandLine(prompt)}\"";

        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, commandLine);

        for (var i = 0; i < count; i++)
        {
            _processLauncher.StartProcess(terminalExe, args);
        }

        await Task.CompletedTask;
        IsOpenCodePanelOpen = false;
    }

    private static string EscapeForCommandLine(string value)
        => value.Replace("\"", "\\\"");

    // --- Tag management ---

    [RelayCommand]
    private async Task AddTagAsync(Repo? repo)
    {
        if (repo is null) return;
        var tag = (NewTagText ?? string.Empty).Trim();
        NewTagText = string.Empty;
        if (string.IsNullOrEmpty(tag)) return;
        await _repoService.AddTagAsync(repo, tag);
    }

    /// <summary>
    /// Adds a specific tag name (e.g. from a quick-add chip in the flyout) to a repo.
    /// Parameter is a <see cref="Tuple{T1, T2}"/> of (Repo, tag-name).
    /// </summary>
    [RelayCommand]
    private async Task AddTagByNameAsync(Tuple<Repo, string>? repoAndTag)
    {
        if (repoAndTag is null) return;
        await _repoService.AddTagAsync(repoAndTag.Item1, repoAndTag.Item2);
    }

    [RelayCommand]
    private async Task RemoveTagAsync(RepoTag? repoTag)
    {
        if (repoTag is null) return;
        await _repoService.RemoveTagAsync(repoTag.Repo, repoTag.Name);
    }

    [RelayCommand]
    private Task ToggleFavoriteAsync(Repo? repo)
    {
        if (repo is null) return Task.CompletedTask;
        return _repoService.ToggleFavoriteAsync(repo);
    }

    // --- Settings & refresh ---

    [RelayCommand]
    private async Task RefreshAsync()
    {
        await _repoService.RefreshAsync(_reposSettings);
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
            await _settingsService.SaveSettingsAsync(settings);

            _reposSettings = edited;
            await _repoService.RefreshAsync(_reposSettings);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error opening repo settings");
        }
    }
}
