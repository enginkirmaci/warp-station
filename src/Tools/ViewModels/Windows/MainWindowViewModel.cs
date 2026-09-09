using System.IO;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Helpers;
using Tools.Library.Configuration;
using Tools.Library.Mvvm;
using Tools.Library.Services;
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
    private readonly IToolDrawerService _toolDrawer;

    /// <summary>
    /// Gets the title of the application.
    /// </summary>
    public string ApplicationTitle { get; } = "Dev Tools";

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

    // ---- Tool drawer (floating right sidebar) ----

    /// <summary>Clamps for the drawer card's resizable width: narrow enough to keep the
    /// page behind usable, wide enough for the two-column settings grids.</summary>
    private const double MinToolDrawerWidth = 420;
    private const double MaxToolDrawerWidth = 800;

    /// <summary>
    /// The drawer card's width in logical pixels, resized via its left-edge grip (same
    /// three-dots divider as the bottom panels, rotated vertical). Session-only —
    /// matching the bottom panels' runtime-only heights.
    /// </summary>
    [ObservableProperty]
    private double _toolDrawerWidth = 540;

    /// <summary>Applies a drag delta (positive = wider) to the drawer width.</summary>
    public void AdjustToolDrawerWidth(double delta)
    {
        // Whole logical pixels only: sub-pixel widths re-rasterize without a visible
        // gain, and unchanged values must not trigger another layout pass (drag smoothness).
        var value = Math.Clamp(Math.Round(ToolDrawerWidth + delta), MinToolDrawerWidth, MaxToolDrawerWidth);
        if (Math.Abs(value - ToolDrawerWidth) < 0.5) return;
        ToolDrawerWidth = value;
    }

    /// <summary>Whether the tool drawer is currently shown over the page.</summary>
    [ObservableProperty]
    private bool _isToolDrawerOpen;

    /// <summary>Title of the tool currently shown in the drawer.</summary>
    [ObservableProperty]
    private string _toolDrawerTitle = string.Empty;

    /// <summary>SVG path data of the current tool's icon, for the drawer header.</summary>
    [ObservableProperty]
    private string _toolDrawerIconPath = string.Empty;

    /// <summary>
    /// Whether the Clipboard Password tool may appear in the GUI. Mirrors the
    /// EnableClipboardPassword setting used to filter the tools dropdown; when
    /// disabled the tool stays reachable through its hotkey only. Loaded
    /// asynchronously after construction (see <see cref="LoadVisibilityFlagsAsync"/>);
    /// the default matches the value computed from default settings (the setting
    /// defaults to <c>false</c> = concealed), so the menu item only ever settles,
    /// never flips, for default configurations.
    /// </summary>
    [ObservableProperty]
    private bool _showClipboardPassword;

    /// <summary>
    /// Whether the NuGet Local tool shows in the GUI: the tools dropdown entry and the
    /// title-bar watch chip mirror <see cref="NugetLocalSettings.EnableNuget"/>. Loaded
    /// asynchronously after construction (see <see cref="LoadVisibilityFlagsAsync"/>)
    /// and re-checked on every NuGet service StateChanged, so a save in the Repo
    /// Settings drawer (which nudges the service) flips the surfaces live.
    /// </summary>
    [ObservableProperty]
    private bool _showNuget = true;

    /// <summary>
    /// SnapIt is Windows-only functionality (the engine is Win32-based), so every
    /// SnapIt surface — the title-bar status chip and the tools dropdown entry —
    /// hides on other platforms. Runtime check, not the build-machine WINDOWS
    /// constant, so Windows-targeted builds still show it when run on Windows.
    /// </summary>
    public bool ShowSnapIt => OperatingSystem.IsWindows();

    public MainWindowViewModel(
        ISnapItService snapItService,
        INugetLocalService nugetLocalService,
        ISettingsService settingsService,
        IProcessLauncher processLauncher,
        IToolDrawerService toolDrawer)
    {
        _snapItService = snapItService;
        _nugetLocalService = nugetLocalService;
        _settingsService = settingsService;
        _processLauncher = processLauncher;
        _toolDrawer = toolDrawer;

        // Load the dropdown visibility flags off the constructor path: the FIRST
        // GetSettingsAsync() call runs EnsureLoaded inline (seed copy, File.ReadAllText,
        // legacy-key migration and a full JSON deserialize), which used to block window
        // construction on disk I/O. The flags only feed IsVisible bindings on the tools
        // dropdown, so the bindings pick the real values up the moment the load
        // completes — worst case an entry shows its default state for a few milliseconds.
        _ = LoadVisibilityFlagsAsync();

        _toolDrawer.Changed += OnToolDrawerChanged;

        ToggleSnapItCommand = new AsyncRelayCommand(OnToggleSnapItAsync);
        ToggleNugetWatchCommand = new AsyncRelayCommand(OnToggleNugetWatchAsync);

        _snapItService.RunningChanged += OnSnapItRunningChanged;
        _nugetLocalService.StateChanged += OnNugetLocalStateChanged;

        UpdateSnapItStatus(_snapItService.IsRunning);
        UpdateNugetWatchStatus();
    }

    /// <summary>
    /// Reads the settings-driven dropdown visibility flags without blocking construction:
    /// the awaited load completes off the UI thread if the constructor ran before the UI
    /// <c>SynchronizationContext</c> existed, so the observable writes are marshaled
    /// through the dispatcher (same discipline as the service event handlers above).
    /// Fire-and-forget from the constructor; failures keep the defaults rather than
    /// crashing startup.
    /// </summary>
    private async Task LoadVisibilityFlagsAsync()
    {
        try
        {
            var appSettings = await _settingsService.GetSettingsAsync();
            Avalonia.Threading.Dispatcher.UIThread.Post(() =>
            {
                ShowClipboardPassword = appSettings.ClipboardPassword?.EnableClipboardPassword == true;
                ShowNuget = appSettings.NugetLocal?.EnableNuget != false;
            });
        }
        catch (Exception ex)
        {
            Serilog.Log.Logger.Error(ex, "Failed to load settings for the main window visibility flags");
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
        // The enable flag rides along: the settings save nudges the service, whose
        // StateChanged lands here, so the menu entry and chip follow the flag live.
        ShowNuget = _nugetLocalService.IsEnabled;
        NugetWatchRunning = _nugetLocalService.IsWatching;
        NugetWatchCount = _nugetLocalService.Count;
        NugetWatchStatusText = _nugetLocalService.IsWatching
            ? (_nugetLocalService.Count > 0 ? $"Watching ({_nugetLocalService.Count})" : "Watching")
            : "Idle";
    }

    private void OnToolDrawerChanged()
    {
        IsToolDrawerOpen = _toolDrawer.IsOpen;

        var tool = ToolComponentMapper.Find(_toolDrawer.SelectedToolKey);
        ToolDrawerTitle = tool?.Title ?? string.Empty;
        ToolDrawerIconPath = tool is null ? string.Empty : IconAssetLoader.GetPathData(tool.IconAsset);
    }

    /// <summary>
    /// Opens a tool component in the floating right drawer. Used by the title-bar
    /// tools dropdown; the drawer service no-ops when that tool is already showing.
    /// </summary>
    [RelayCommand]
    private void OpenTool(string? toolKey)
    {
        if (!string.IsNullOrWhiteSpace(toolKey))
        {
            _toolDrawer.Open(toolKey);
        }
    }

    /// <summary>Closes the tool drawer (header ✕ button).</summary>
    [RelayCommand]
    private void CloseToolDrawer()
    {
        _toolDrawer.Close();
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
