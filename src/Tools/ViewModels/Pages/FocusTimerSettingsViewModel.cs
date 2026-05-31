using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;
using Tools.Views.Windows;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Focus Timer Settings page.
/// </summary>
public partial class FocusTimerSettingsViewModel : PageViewModelBase
{
    #region Fields

    private readonly ISettingsService _settingsService;
    private readonly IFocusTimerService _focusTimerService;

    private FocusTimerSettings _settings = new();

    // Guard to avoid saving settings while loading/initializing
    private bool _isInitializing;

    #endregion Fields

    #region Observable Properties - Work Window

    [ObservableProperty]
    private TimeSpan _workStartTime = new(9, 0, 0);

    [ObservableProperty]
    private TimeSpan _workEndTime = new(18, 0, 0);

    #endregion Observable Properties - Work Window

    #region Observable Properties - Lunch Settings

    [ObservableProperty]
    private TimeSpan _lunchStartTime = new(13, 0, 0);

    [ObservableProperty]
    private int _lunchDurationMinutes = 60;

    #endregion Observable Properties - Lunch Settings

    #region Observable Properties - Break Goals

    [ObservableProperty]
    private int _totalDailyBreakMinutes = 60;

    [ObservableProperty]
    private int _desiredBreakCount = 4;

    #endregion Observable Properties - Break Goals

    #region Observable Properties - Display Options

    [ObservableProperty]
    private int _selectedVisibilityMode;

    [ObservableProperty]
    private int _selectedCornerPosition;

    [ObservableProperty]
    private bool _playSoundOnNotification = true;

    #endregion Observable Properties - Display Options

    #region Observable Properties - Timer State

    [ObservableProperty]
    private bool _isTimerRunning;

    [ObservableProperty]
    private string _timerStatusText = "Timer is stopped";

    [ObservableProperty]
    private double _currentBreakPool;

    [ObservableProperty]
    private int _remainingBreaks;

    [ObservableProperty]
    private double _breaksTakenToday;

    [ObservableProperty]
    private string _calculatedBreakDuration = "15 min";

    [ObservableProperty]
    private string _calculatedBreakInterval = "Every 2 hours";

    // Formatted display properties for binding in XAML
    [ObservableProperty]
    private string _currentBreakPoolText = "0";

    [ObservableProperty]
    private string _breaksTakenTodayText = "0";

    #endregion Observable Properties - Timer State

    #region Lookup Collections

    public List<string> VisibilityModes { get; } =
    [
        "Always visible",
        "Only on notification"
    ];

    public List<string> CornerPositions { get; } =
    [
        "Bottom Right",
        "Bottom Left",
        "Top Right",
        "Top Left"
    ];

    #endregion Lookup Collections

    #region Commands

    public IAsyncRelayCommand StartTimerCommand { get; }
    public IAsyncRelayCommand StopTimerCommand { get; }
    public IRelayCommand ResetDailyStateCommand { get; }
    public IRelayCommand ShowTimerWindowCommand { get; }

    #endregion Commands

    #region Events

    /// <summary>
    /// Occurs when the timer window should be shown.
    /// </summary>
    public event EventHandler? ShowTimerWindowRequested;

    #endregion Events

    #region Constructor

    public FocusTimerSettingsViewModel(
        ISettingsService settingsService,
        IFocusTimerService focusTimerService)
    {
        _settingsService = settingsService;
        _focusTimerService = focusTimerService;


        StartTimerCommand = new AsyncRelayCommand(OnStartTimerAsync, CanStartTimer);
        StopTimerCommand = new AsyncRelayCommand(OnStopTimerAsync, CanStopTimer);
        ResetDailyStateCommand = new RelayCommand(OnResetDailyState);
        ShowTimerWindowCommand = new RelayCommand(OnShowTimerWindow);

        // Subscribe to service events
        _focusTimerService.StateChanged += OnServiceStateChanged;

        _ = OnInitializeAsync();
    }

    #endregion Constructor

    #region Lifecycle

    public override async Task OnInitializeAsync()
    {
        _isInitializing = true;
        try
        {
            await LoadSettingsAsync();
        }
        finally
        {
            _isInitializing = false;
        }

        UpdateTimerState();
        UpdateCalculatedValues();
    }

    #endregion Lifecycle

    #region Private Methods - Settings

    private async Task LoadSettingsAsync()
    {
        var appSettings = await _settingsService.GetSettingsAsync();
        _settings = appSettings.FocusTimer ?? new FocusTimerSettings();

        // Populate UI from settings
        WorkStartTime = ParseTimeSpan(_settings.WorkStartTime);
        WorkEndTime = ParseTimeSpan(_settings.WorkEndTime);
        LunchStartTime = ParseTimeSpan(_settings.LunchStartTime);
        LunchDurationMinutes = _settings.LunchDurationMinutes;
        TotalDailyBreakMinutes = _settings.TotalDailyBreakMinutes;
        DesiredBreakCount = _settings.DesiredBreakCount;
        SelectedVisibilityMode = _settings.TimerVisibilityMode;
        SelectedCornerPosition = _settings.WindowCornerPosition;
        PlaySoundOnNotification = _settings.PlaySoundOnNotification;

        // Load persisted state
        if (_settings.PersistedState != null)
        {
            CurrentBreakPool = _settings.PersistedState.CurrentBreakPoolMinutes;
            RemainingBreaks = _settings.PersistedState.RemainingBreakCount;
            BreaksTakenToday = _settings.PersistedState.BreakTimeTakenMinutes;
        }
    }

    private async Task SaveSettingsAsync()
    {
        if (_isInitializing) return;

        UpdateSettingsFromProperties();
        await _focusTimerService.UpdateSettingsAsync(_settings);
        UpdateCalculatedValues();
    }

    private void UpdateSettingsFromProperties()
    {
        _settings.WorkStartTime = FormatTimeSpan(WorkStartTime);
        _settings.WorkEndTime = FormatTimeSpan(WorkEndTime);
        _settings.LunchStartTime = FormatTimeSpan(LunchStartTime);
        _settings.LunchDurationMinutes = LunchDurationMinutes;
        _settings.TotalDailyBreakMinutes = TotalDailyBreakMinutes;
        _settings.DesiredBreakCount = DesiredBreakCount;
        _settings.TimerVisibilityMode = SelectedVisibilityMode;
        _settings.WindowCornerPosition = SelectedCornerPosition;
        _settings.PlaySoundOnNotification = PlaySoundOnNotification;
    }

    private static TimeSpan ParseTimeSpan(string timeString)
    {
        if (TimeSpan.TryParse(timeString, out var time))
        {
            return time;
        }
        return new TimeSpan(9, 0, 0);
    }

    private static string FormatTimeSpan(TimeSpan time)
    {
        return $"{time.Hours:D2}:{time.Minutes:D2}";
    }

    #endregion Private Methods - Settings

    #region Private Methods - Timer Control

    private async Task OnStartTimerAsync()
    {
        await _focusTimerService.StartAsync();
        UpdateTimerState();
    }

    private bool CanStartTimer()
    {
        return !_focusTimerService.IsRunning;
    }

    private async Task OnStopTimerAsync()
    {
        await _focusTimerService.StopAsync();
        UpdateTimerState();
    }

    private bool CanStopTimer()
    {
        return _focusTimerService.IsRunning;
    }

    private void OnResetDailyState()
    {
        // Reset the break pool to full
        CurrentBreakPool = TotalDailyBreakMinutes;
        RemainingBreaks = DesiredBreakCount;
        BreaksTakenToday = 0;

        // Update settings persisted state
        _settings.PersistedState = new FocusTimerPersistedState
        {
            LastResetDate = DateTime.Today.ToString("yyyy-MM-dd"),
            CurrentBreakPoolMinutes = TotalDailyBreakMinutes,
            RemainingBreakCount = DesiredBreakCount,
            BreakTimeTakenMinutes = 0
        };

        _ = SaveSettingsAsync();

        // Recalculate if timer is running
        if (_focusTimerService.IsRunning)
        {
            _focusTimerService.RecalculateSchedule();
        }
    }

    private void OnShowTimerWindow()
    {
        ShowTimerWindowRequested?.Invoke(this, EventArgs.Empty);
    }

    private void UpdateTimerState()
    {
        IsTimerRunning = _focusTimerService.IsRunning;
        var state = _focusTimerService.CurrentState;

        TimerStatusText = state.Status switch
        {
            FocusTimerStatus.Stopped => "Timer is stopped",
            FocusTimerStatus.Working => state.TimeUntilNextBreak.HasValue ? $"Working - Next break: {FormatTimeSpan(state.TimeUntilNextBreak.Value)}" : "Working...",
            FocusTimerStatus.NotificationTriggered => "Break time!",
            FocusTimerStatus.BreakActive => state.BreakTimeRemaining.HasValue ? $"On break - {FormatTimeSpan(state.BreakTimeRemaining.Value)} remaining" : "On break",
            FocusTimerStatus.LunchMode => "Lunch break",
            FocusTimerStatus.DayEnded => "Work day ended",
            _ => "Unknown"
        };

        CurrentBreakPool = state.CurrentBreakPoolMinutes;
        RemainingBreaks = state.RemainingBreakCount;
        BreaksTakenToday = state.BreakTimeTakenMinutes;

        StartTimerCommand.NotifyCanExecuteChanged();
        StopTimerCommand.NotifyCanExecuteChanged();
    }

    private void UpdateCalculatedValues()
    {
        UpdateSettingsFromProperties();

        var breakDuration = GetCalculatedBreakDuration();
        CalculatedBreakDuration = $"{breakDuration} min per break";

        var (hours, minutes) = GetCalculatedBreakInterval();

        if (hours >= 1)
        {
            CalculatedBreakInterval = $"Approximately every {hours:F1} hours";
        }
        else
        {
            CalculatedBreakInterval = $"Approximately every {minutes:F0} minutes";
        }
    }

    private int GetCalculatedBreakDuration()
    {
        return DesiredBreakCount > 0
            ? TotalDailyBreakMinutes / DesiredBreakCount
            : TotalDailyBreakMinutes;
    }

    private (double hours, double minutes) GetCalculatedBreakInterval()
    {
        var workStart = WorkStartTime;
        var workEnd = WorkEndTime;
        var lunchDuration = TimeSpan.FromMinutes(LunchDurationMinutes);
        var totalWorkTime = workEnd - workStart - lunchDuration;

        var intervalMinutes = DesiredBreakCount > 0
            ? totalWorkTime.TotalMinutes / (DesiredBreakCount + 1)
            : totalWorkTime.TotalMinutes;

        return (intervalMinutes / 60, intervalMinutes % 60);
    }

    #endregion Private Methods - Timer Control

    #region Event Handlers

    private void OnServiceStateChanged(object? sender, FocusTimerStateChangedEventArgs e)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            UpdateTimerState();
        });
    }

    #endregion Event Handlers

    #region Property Change Handlers

    private void OnSettingChanged() => _ = SaveSettingsAsync();

    partial void OnWorkStartTimeChanged(TimeSpan value) => OnSettingChanged();
    partial void OnWorkEndTimeChanged(TimeSpan value) => OnSettingChanged();
    partial void OnLunchStartTimeChanged(TimeSpan value) => OnSettingChanged();
    partial void OnLunchDurationMinutesChanged(int value) => OnSettingChanged();
    partial void OnTotalDailyBreakMinutesChanged(int value) => OnSettingChanged();
    partial void OnDesiredBreakCountChanged(int value) => OnSettingChanged();
    partial void OnSelectedVisibilityModeChanged(int value) => OnSettingChanged();
    partial void OnSelectedCornerPositionChanged(int value) => OnSettingChanged();
    partial void OnPlaySoundOnNotificationChanged(bool value) => OnSettingChanged();

    // Update formatted text when numeric values change
    partial void OnCurrentBreakPoolChanged(double value)
    {
        CurrentBreakPoolText = value.ToString("F0");
    }

    partial void OnBreaksTakenTodayChanged(double value)
    {
        BreaksTakenTodayText = value.ToString("F0");
    }

    #endregion Property Change Handlers
}