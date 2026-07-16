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
    private readonly IOpenCodeServeService _openCodeServeService;
    private readonly ISettingsService _settingsService;
    private readonly INotificationService _notificationService;
    private OpenCodeServeSettings _openCodeServeSettings = new();

    /// <summary>
    /// Tracks whether the serve connection change came from an explicit user toggle (vs. the
    /// auto-start at app launch), so launch-time connects don't fire a toast.
    /// </summary>
    private bool _isExplicitServeToggle;

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

    // ---- Status bar: OpenCode serve ----
    [ObservableProperty]
    private bool _openCodeServeConnected;

    [ObservableProperty]
    private string _openCodeServeStatusText = "Disconnected";

    /// <summary>Command to start/stop the managed opencode serve subprocess from the status bar.</summary>
    public IAsyncRelayCommand ToggleOpenCodeServeCommand { get; }

    // ---- Left navigation sidebar visibility ----
    /// <summary>Tracks whether the left navigation sidebar is shown.</summary>
    [ObservableProperty]
    private bool _isLeftSidebarOpen = true;

    /// <summary>Command to toggle the left navigation sidebar open/closed.</summary>
    public IRelayCommand ToggleLeftSidebarCommand { get; }

    public MainWindowViewModel(
        ISnapItService snapItService,
        INugetLocalService nugetLocalService,
        IOpenCodeServeService openCodeServeService,
        ISettingsService settingsService,
        INotificationService notificationService)
    {
        _snapItService = snapItService;
        _nugetLocalService = nugetLocalService;
        _openCodeServeService = openCodeServeService;
        _settingsService = settingsService;
        _notificationService = notificationService;

        // Read the hide flag and opencode serve settings synchronously: GetSettingsAsync is an
        // in-memory cached read (Task.FromResult), so this never blocks on async work.
        var appSettings = settingsService.GetSettingsAsync().GetAwaiter().GetResult();
        var hideClipboardPassword = appSettings.ClipboardPassword?.HideFromGui == true;
        _openCodeServeSettings = appSettings.OpenCode ?? new OpenCodeServeSettings();
        MenuItems = NavigationProvider.GetNavigationMenuItems(hideClipboardPassword);

        ToggleSnapItCommand = new AsyncRelayCommand(OnToggleSnapItAsync);
        ToggleNugetWatchCommand = new AsyncRelayCommand(OnToggleNugetWatchAsync);
        ToggleOpenCodeServeCommand = new AsyncRelayCommand(OnToggleOpenCodeServeAsync);
        ToggleLeftSidebarCommand = new RelayCommand(OnToggleLeftSidebar);

        _snapItService.RunningChanged += OnSnapItRunningChanged;
        _nugetLocalService.StateChanged += OnNugetLocalStateChanged;
        _openCodeServeService.ConnectionChanged += OnOpenCodeServeConnectionChanged;

        UpdateSnapItStatus(_snapItService.IsRunning);
        UpdateNugetWatchStatus();
        UpdateOpenCodeServeStatus(_openCodeServeService.IsConnected);

        // Auto-start serve at app launch when configured (so it's ready before the user opens Repos).
        if (_openCodeServeSettings.AutoConnect)
        {
            _ = _openCodeServeService.EnsureStartedAsync(_openCodeServeSettings);
        }
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

    private void OnToggleLeftSidebar() => IsLeftSidebarOpen = !IsLeftSidebarOpen;

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
    /// Toggles the managed opencode serve subprocess. Starting reads the latest serve settings
    /// (so a settings-dialog edit is picked up); stopping tears the subprocess down.
    /// </summary>
    private async Task OnToggleOpenCodeServeAsync()
    {
        _isExplicitServeToggle = true;
        if (_openCodeServeService.IsConnected)
        {
            _openCodeServeService.Stop();
            UpdateOpenCodeServeStatus(false);
        }
        else
        {
            // Pick up any settings changes since construction.
            var appSettings = await _settingsService.GetSettingsAsync();
            _openCodeServeSettings = appSettings.OpenCode ?? new OpenCodeServeSettings();
            await _openCodeServeService.EnsureStartedAsync(_openCodeServeSettings, force: true);
            UpdateOpenCodeServeStatus(_openCodeServeService.IsConnected);
        }
        _isExplicitServeToggle = false;
    }

    private void OnOpenCodeServeConnectionChanged(object? sender, bool isConnected)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            UpdateOpenCodeServeStatus(isConnected);

            // Only toast on explicit user toggles, not the app-launch auto-start.
            if (_isExplicitServeToggle)
            {
                _notificationService.Show(
                    isConnected ? "OpenCode serve connected" : "OpenCode serve disconnected",
                    isConnected ? NotificationKind.Success : NotificationKind.Info);
            }
        });
    }

    private void UpdateOpenCodeServeStatus(bool isConnected)
    {
        OpenCodeServeConnected = isConnected;
        OpenCodeServeStatusText = isConnected
            ? $"serve :{_openCodeServeSettings.Port}"
            : "Disconnected";
    }
}
