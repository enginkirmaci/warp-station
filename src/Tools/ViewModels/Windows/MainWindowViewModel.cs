using System.IO;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Providers;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the main window of the application.
/// </summary>
public partial class MainWindowViewModel : ViewModelBase
{
    private readonly ISnapItService _snapItService;
    private readonly INugetLocalService _nugetLocalService;
    private readonly ISettingsService _settingsService;
    private readonly IProcessLauncher _processLauncher;

    /// <summary>
    /// Gets the title of the application.
    /// </summary>
    public string ApplicationTitle { get; } = "Dev Tools";

    /// <summary>
    /// Gets the collection of menu items for navigation.
    /// </summary>
    public IReadOnlyCollection<NavigationItem> MenuItems { get; }

    // ---- Status bar: SnapIt ----
    [ObservableProperty]
    private bool _snapItRunning;

    [ObservableProperty]
    private string _snapItStatusText = "Stopped";

    /// <summary>Command to toggle SnapIt on/off from the status bar.</summary>
    public IRelayCommand ToggleSnapItCommand { get; }

    // ---- Status bar: NuGet Local watch ----
    [ObservableProperty]
    private bool _nugetWatchRunning;

    [ObservableProperty]
    private string _nugetWatchStatusText = "Idle";

    [ObservableProperty]
    private int _nugetWatchCount;

    /// <summary>Command to toggle the NuGet local watch from the status bar.</summary>
    public IAsyncRelayCommand ToggleNugetWatchCommand { get; }

    // ---- Left navigation sidebar collapse state ----
    /// <summary>Tracks whether the left navigation sidebar is collapsed to an icon-only rail.</summary>
    [ObservableProperty]
    private bool _isSidebarCollapsed;

    /// <summary>Command to toggle the left navigation sidebar between expanded and icon-only.</summary>
    public IAsyncRelayCommand ToggleSidebarCommand { get; }

    public MainWindowViewModel(
        ISnapItService snapItService,
        INugetLocalService nugetLocalService,
        ISettingsService settingsService,
        IProcessLauncher processLauncher)
    {
        _snapItService = snapItService;
        _nugetLocalService = nugetLocalService;
        _settingsService = settingsService;
        _processLauncher = processLauncher;

        // Read the hide flag synchronously: GetSettingsAsync is an in-memory cached read
        // (Task.FromResult), so this never blocks on async work.
        var appSettings = settingsService.GetSettingsAsync().GetAwaiter().GetResult();
        var hideClipboardPassword = appSettings.ClipboardPassword?.HideFromGui == true;
        MenuItems = NavigationProvider.GetNavigationMenuItems(hideClipboardPassword);

        // Restore the sidebar's last collapse state so the layout matches the
        // previous session. Only the initial value is read sync (cached read);
        // subsequent toggles persist via the async OnToggleSidebar handler.
        IsSidebarCollapsed = appSettings.General?.SidebarCollapsed == true;

        ToggleSnapItCommand = new AsyncRelayCommand(OnToggleSnapItAsync);
        ToggleNugetWatchCommand = new AsyncRelayCommand(OnToggleNugetWatchAsync);
        ToggleSidebarCommand = new AsyncRelayCommand(OnToggleSidebarAsync);

        _snapItService.RunningChanged += OnSnapItRunningChanged;
        _nugetLocalService.StateChanged += OnNugetLocalStateChanged;

        UpdateSnapItStatus(_snapItService.IsRunning);
        UpdateNugetWatchStatus();
    }

    private async Task OnToggleSnapItAsync()
    {
        if (_snapItService.IsRunning)
        {
            _snapItService.Stop();
        }
        else
        {
            await _snapItService.StartAsync();
        }
    }

    private async Task OnToggleSidebarAsync()
    {
        IsSidebarCollapsed = !IsSidebarCollapsed;

        // Persist the new state so it is restored on the next launch. Failures
        // are non-fatal (the in-memory state is already correct), so log and
        // swallow rather than surfacing an error for a UI preference.
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            settings.General ??= new GeneralSettings();
            settings.General.SidebarCollapsed = IsSidebarCollapsed;
            await _settingsService.SaveSettingsAsync(settings);
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"Failed to persist sidebar state: {ex.Message}");
        }
    }

    private async Task OnToggleNugetWatchAsync()
    {
        if (_nugetLocalService.IsWatching)
        {
            _nugetLocalService.Stop();
        }
        else
        {
            await _nugetLocalService.StartAsync();
        }
    }

    private void OnSnapItRunningChanged(object? sender, bool isRunning)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() => UpdateSnapItStatus(isRunning));
    }

    private void OnNugetLocalStateChanged(object? sender, EventArgs e)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(UpdateNugetWatchStatus);
    }

    private void UpdateSnapItStatus(bool isRunning)
    {
        SnapItRunning = isRunning;
        SnapItStatusText = isRunning ? "Running" : "Stopped";
    }

    private void UpdateNugetWatchStatus()
    {
        NugetWatchRunning = _nugetLocalService.IsWatching;
        NugetWatchCount = _nugetLocalService.Count;
        NugetWatchStatusText = _nugetLocalService.IsWatching
            ? (_nugetLocalService.Count > 0 ? $"Watching ({_nugetLocalService.Count})" : "Watching")
            : "Idle";
    }

    /// <summary>
    /// Opens the user settings folder (<c>%USERPROFILE%\.devtools</c>) in the OS
    /// file explorer. ProcessLauncher uses <c>UseShellExecute=true</c>, so passing a folder
    /// path opens it in the default explorer. The folder is created on demand by the
    /// settings service when it persists settings, and is created here as a safety net so
    /// the button always opens something rather than erroring on first use.
    /// </summary>
    [RelayCommand]
    private void OpenSettingsFolder()
    {
        var settingsDirectory = UserPaths.UserDataRoot;
        Directory.CreateDirectory(settingsDirectory);
        _processLauncher.StartProcess(settingsDirectory);
    }
}
