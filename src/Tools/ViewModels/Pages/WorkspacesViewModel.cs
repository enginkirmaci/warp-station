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
/// Thin binding adapter for the Workspaces page. Delegates scanning, caching, and the
/// shared workspace/platform state to <see cref="IWorkspaceService"/> (singleton), and
/// process launching to <see cref="IProcessLauncher"/>. Holds only view-specific state
/// (the filter and its filtered projections).
/// </summary>
public partial class WorkspacesViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly IDevToolsClient _devToolsClient;
    private readonly IDialogService _dialogService;
    private readonly IWorkspaceService _workspaceService;
    private readonly IProcessLauncher _processLauncher;
    private WorkspacesSettings _workspaceSettings = new();

    [ObservableProperty]
    private string _filterText = string.Empty;

    [ObservableProperty]
    private ObservableCollection<WorkspaceItem> _filteredWorkspaces = new();

    [ObservableProperty]
    private ObservableCollection<WorkspaceItem> _filteredPlatforms = new();

    public WorkspacesViewModel(
        ISettingsService settingsService,
        IDevToolsClient devToolsClient,
        IDialogService dialogService,
        IWorkspaceService workspaceService,
        IProcessLauncher processLauncher)
    {
        _settingsService = settingsService;
        _devToolsClient = devToolsClient;
        _dialogService = dialogService;
        _workspaceService = workspaceService;
        _processLauncher = processLauncher;

        _workspaceService.Changed += OnWorkspaceChanged;
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override Task OnNavigatedFromAsync()
    {
        // Detach from the singleton so this Transient VM (rebuilt per navigation) is not
        // kept alive by the service and does not receive further state changes.
        _workspaceService.Changed -= OnWorkspaceChanged;
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        _workspaceSettings = settings.Workspaces ?? new WorkspacesSettings();
        await _workspaceService.EnsureLoadedAsync(_workspaceSettings);
        ApplyFilter();
    }

    private void OnWorkspaceChanged(object? sender, EventArgs e)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            IsBusy = _workspaceService.IsBusy;
            ApplyFilter();
        });
    }

    partial void OnFilterTextChanged(string value) => ApplyFilter();

    [RelayCommand]
    private void ClearFilter() => FilterText = string.Empty;

    private void ApplyFilter()
    {
        var filter = FilterText?.Trim();
        var workspaces = _workspaceService.Workspaces;
        var platforms = _workspaceService.Platforms;

        if (string.IsNullOrWhiteSpace(filter))
        {
            FilteredWorkspaces = new ObservableCollection<WorkspaceItem>(workspaces);
            FilteredPlatforms = new ObservableCollection<WorkspaceItem>(platforms);
        }
        else
        {
            FilteredWorkspaces = new ObservableCollection<WorkspaceItem>(
                workspaces.Where(w => w.SolutionName?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true));
            FilteredPlatforms = new ObservableCollection<WorkspaceItem>(
                platforms.Where(p => p.PlatformName?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true));
        }
    }

    [RelayCommand]
    private void OpenSolution(string? solutionPath) => _processLauncher.StartProcess(solutionPath);

    [RelayCommand]
    private void OpenFolder(string? folderPath) => _processLauncher.StartProcess(folderPath);

    [RelayCommand]
    private async Task OpenWithVSCodeAsync(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = _workspaceSettings.VSCodeExecutable ?? "code";

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

        var exe = _workspaceSettings.TerminalExecutable ?? "wt";
        var args = TerminalArgumentFormatter.BuildArguments(exe, folderPath);
        _processLauncher.StartProcess(exe, args);
    }

    [RelayCommand]
    private async Task RefreshAsync()
    {
        await _workspaceService.RefreshAsync(_workspaceSettings);
    }

    [RelayCommand]
    private async Task OpenSettingsAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var currentWorkspaceSettings = settings.Workspaces ?? new WorkspacesSettings();

            var edited = await _dialogService.ShowWorkspaceSettingsDialogAsync(currentWorkspaceSettings);
            if (edited == null)
            {
                // User cancelled the dialog.
                return;
            }

            settings.Workspaces = edited;
            await _settingsService.SaveSettingsAsync(settings);

            _workspaceSettings = edited;
            await _workspaceService.RefreshAsync(_workspaceSettings);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error opening workspace settings");
        }
    }
}
