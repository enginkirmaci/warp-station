using System.Collections.ObjectModel;
using System.Text;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Providers;
using Tools.Library.Services.Abstractions;
using Tools.Library.Services.Logging;

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
    private readonly MemoryLogSink _memoryLogSink;
    private readonly IClipboardService _clipboardService;
    private OpenCodeServeSettings _openCodeServeSettings = new();

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

    // ---- Log panel ----

    /// <summary>
    /// The log entries shown in the log sidebar, newest-last. Fed by <see cref="MemoryLogSink"/>.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<LogEntry> _logEntries = new();

    /// <summary>Whether the log sidebar is open.</summary>
    [ObservableProperty]
    private bool _isLogPanelOpen;

    /// <summary>Command to toggle the log sidebar open/closed.</summary>
    public IRelayCommand ToggleLogPanelCommand { get; }

    /// <summary>Command to clear all retained log entries.</summary>
    public IRelayCommand ClearLogCommand { get; }

    /// <summary>Command to copy all retained log entries to the clipboard.</summary>
    public IRelayCommand CopyLogsCommand { get; }

    public MainWindowViewModel(
        ISnapItService snapItService,
        INugetLocalService nugetLocalService,
        IOpenCodeServeService openCodeServeService,
        ISettingsService settingsService,
        MemoryLogSink memoryLogSink,
        IClipboardService clipboardService)
    {
        _snapItService = snapItService;
        _nugetLocalService = nugetLocalService;
        _openCodeServeService = openCodeServeService;
        _settingsService = settingsService;
        _memoryLogSink = memoryLogSink;
        _clipboardService = clipboardService;

        // Read the hide flag and opencode serve settings synchronously: GetSettingsAsync is an
        // in-memory cached read (Task.FromResult), so this never blocks on async work.
        var appSettings = settingsService.GetSettingsAsync().GetAwaiter().GetResult();
        var hideClipboardPassword = appSettings.ClipboardPassword?.HideFromGui == true;
        _openCodeServeSettings = appSettings.OpenCode ?? new OpenCodeServeSettings();
        MenuItems = NavigationProvider.GetNavigationMenuItems(hideClipboardPassword);

        ToggleSnapItCommand = new AsyncRelayCommand(OnToggleSnapItAsync);
        ToggleNugetWatchCommand = new AsyncRelayCommand(OnToggleNugetWatchAsync);
        ToggleOpenCodeServeCommand = new AsyncRelayCommand(OnToggleOpenCodeServeAsync);
        ToggleLogPanelCommand = new RelayCommand(OnToggleLogPanel);
        ClearLogCommand = new RelayCommand(OnClearLog);
        CopyLogsCommand = new RelayCommand(OnCopyLogs);

        _snapItService.RunningChanged += OnSnapItRunningChanged;
        _nugetLocalService.StateChanged += OnNugetLocalStateChanged;
        _openCodeServeService.ConnectionChanged += OnOpenCodeServeConnectionChanged;
        _memoryLogSink.EntryAppended += OnLogEntryAppended;

        UpdateSnapItStatus(_snapItService.IsRunning);
        UpdateNugetWatchStatus();
        UpdateOpenCodeServeStatus(_openCodeServeService.IsConnected);

        // Seed the panel with anything already captured (e.g. logs emitted before the VM was built).
        LogEntries = new ObservableCollection<LogEntry>(_memoryLogSink.Entries);

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
    }

    private void OnOpenCodeServeConnectionChanged(object? sender, bool isConnected)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() => UpdateOpenCodeServeStatus(isConnected));
    }

    private void UpdateOpenCodeServeStatus(bool isConnected)
    {
        OpenCodeServeConnected = isConnected;
        OpenCodeServeStatusText = isConnected
            ? $"serve :{_openCodeServeSettings.Port}"
            : "Disconnected";
    }

    // ---- Log panel ----

    private void OnToggleLogPanel() => IsLogPanelOpen = !IsLogPanelOpen;

    private void OnClearLog()
    {
        _memoryLogSink.Clear();
        LogEntries.Clear();
    }

    /// <summary>
    /// Copies all retained log entries to the clipboard, formatted one per line with the
    /// timestamp, level, message, and (when present) exception — mirroring the panel layout.
    /// </summary>
    private void OnCopyLogs()
    {
        if (LogEntries.Count == 0)
            return;

        var sb = new StringBuilder();
        foreach (var entry in LogEntries)
        {
            sb.Append(entry.Timestamp.ToLocalTime().ToString("yyyy-MM-dd HH:mm:ss"));
            sb.Append(' ').Append(entry.LevelText).Append(' ').Append(entry.Message);
            if (!string.IsNullOrEmpty(entry.Exception))
                sb.Append(Environment.NewLine).Append(entry.Exception);
            sb.Append(Environment.NewLine);
        }

        _clipboardService.CopyText(sb.ToString().TrimEnd());
    }

    /// <summary>
    /// Appends a log entry to <see cref="LogEntries"/> on the UI thread. The sink raises this on
    /// a background (Serilog pipeline) thread, so we marshal to the UI thread.
    /// </summary>
    private void OnLogEntryAppended(LogEntry entry)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() => LogEntries.Add(entry));
    }
}
