using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using Tools.Library.Services.States;

namespace Tools.Library.Services;

/// <summary>
/// Implements the Focus Timer with Dynamic Rebalancing Algorithm.
/// </summary>
public class FocusTimerService : IFocusTimerService
{
    #region Constants

    private const int TimerIntervalMs = 1000; // 1 second tick

    #endregion Constants

    #region Fields

    private readonly ISettingsService _settingsService;
    private readonly IFocusTimerScheduler _scheduler;
    private readonly IFocusTimerStateManager _stateManager;
    private readonly ITimerProvider _timerProvider;
    private readonly ITimerNotificationService _notificationService;
    private FocusTimerSettings _settings = new();
    private FocusTimerState _state = new();
    private bool _isRunning;
    private IFocusTimerState _currentState = new StoppedState();

    #endregion Fields

    #region Events

    public event EventHandler<FocusTimerStateChangedEventArgs>? StateChanged;

    public event EventHandler? BreakNotificationTriggered;

    public event EventHandler? BreakStarted;

    public event EventHandler? BreakEnded;

    public event EventHandler<bool>? WindowVisibilityRequested;

    public event EventHandler? SettingsChanged;

    #endregion Events

    #region Properties

    public FocusTimerState CurrentState => _state;
    public FocusTimerSettings Settings => _settings;
    public bool IsRunning => _isRunning;

    internal IFocusTimerScheduler Scheduler => _scheduler;

    #endregion Properties

    #region Constructor

    public FocusTimerService(
        ISettingsService settingsService,
        IFocusTimerScheduler scheduler,
        IFocusTimerStateManager stateManager,
        ITimerProvider timerProvider,
        ITimerNotificationService notificationService)
    {
        _settingsService = settingsService;
        _scheduler = scheduler;
        _stateManager = stateManager;
        _timerProvider = timerProvider;
        _notificationService = notificationService;

        _timerProvider.Tick += OnTimerTick;
    }

    #endregion Constructor

    #region Public Methods

    public async Task InitializeAsync()
    {
        var appSettings = await _settingsService.GetSettingsAsync();
        _settings = appSettings.FocusTimer ?? new FocusTimerSettings();

        // Check for daily reset and load state
        _state = await _stateManager.LoadStateAsync(_settings);
    }

    public async Task StartAsync()
    {
        if (_isRunning) return;

        await InitializeAsync();

        _isRunning = true;
        _state.TotalDailyBreakMinutes = _settings.TotalDailyBreakMinutes;

        // Initialize break pool if starting fresh
        if (_state.CurrentBreakPoolMinutes <= 0 && _state.RemainingBreakCount <= 0)
        {
            _state.CurrentBreakPoolMinutes = _settings.TotalDailyBreakMinutes;
            _state.RemainingBreakCount = _settings.DesiredBreakCount;
        }

        SetState(new WorkingState());

        // Request window visibility based on settings
        var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        Debug.WriteLine($"[FocusTimerService] StartAsync: Requesting window visibility. Mode={visibilityMode}, ShouldShow={visibilityMode == TimerVisibilityMode.Always}");
        RequestVisibility(visibilityMode == TimerVisibilityMode.Always);

        Debug.WriteLine($"[FocusTimerService] StartAsync: Starting timer. NextBreakTime={_state.NextBreakTime:HH:mm}, TimeUntilNextBreak={_state.TimeUntilNextBreak?.TotalMinutes:F1}min");
        _timerProvider.Start(TimeSpan.FromMilliseconds(TimerIntervalMs));

        // Fire initial state change to update UI with calculated schedule
        Debug.WriteLine($"[FocusTimerService] StartAsync: Firing initial StateChanged event. Status={_state.Status}");
        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, FocusTimerStatus.Stopped));

        await _stateManager.PersistStateAsync(_state);
    }

    public async Task StopAsync()
    {
        if (!_isRunning) return;

        _isRunning = false;
        _timerProvider.Stop();
        SetState(new StoppedState());
        RequestVisibility(false);
        await _stateManager.PersistStateAsync(_state);
    }

    public void TakeBreak()
    {
        if (_state.Status != FocusTimerStatus.NotificationTriggered) return;

        _state.CurrentBreakStartTime = DateTime.Now;
        _state.BreakTimeRemaining = TimeSpan.FromMinutes(_state.CurrentBreakDurationMinutes);
        SetState(new BreakActiveState());
        BreakStarted?.Invoke(this, EventArgs.Empty);
    }

    public void TakeBreakNow()
    {
        if (_state.Status != FocusTimerStatus.Working && _state.Status != FocusTimerStatus.LunchMode) return;

        // Allow user to take a break manually during work time
        // Use the calculated break duration or a default if not available
        var breakDuration = _state.CurrentBreakDurationMinutes > 0
            ? _state.CurrentBreakDurationMinutes
            : 15; // Default 15 minutes

        _state.CurrentBreakStartTime = DateTime.Now;
        _state.BreakTimeRemaining = TimeSpan.FromMinutes(breakDuration);
        _state.CurrentBreakDurationMinutes = breakDuration;
        SetState(new BreakActiveState());
        BreakStarted?.Invoke(this, EventArgs.Empty);
    }

    public void SnoozeBreak(int minutes = 5)
    {
        if (_state.Status != FocusTimerStatus.NotificationTriggered) return;

        _state.SnoozedUntil = DateTime.Now.AddMinutes(minutes);
        SetState(new WorkingState());

        // Keep window visible during snooze if in "always" mode
        var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        if (visibilityMode == TimerVisibilityMode.OnNotificationOnly)
        {
            RequestVisibility(false);
        }
    }

    public void EndBreakEarly()
    {
        if (_state.Status != FocusTimerStatus.BreakActive) return;

        CompleteBreakInternal(isEarly: true);
    }

    public void RecalculateSchedule()
    {
        _scheduler.Recalculate(_state, _settings);
    }

    public async Task UpdateSettingsAsync(FocusTimerSettings settings)
    {
        var oldVisibility = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        _settings = settings;
        var newVisibility = (TimerVisibilityMode)_settings.TimerVisibilityMode;

        var appSettings = await _settingsService.GetSettingsAsync();
        appSettings.FocusTimer = settings;
        await _settingsService.SaveSettingsAsync(appSettings);

        if (_isRunning)
        {
            _scheduler.Recalculate(_state, _settings);

            // Handle visibility change if it was modified
            if (oldVisibility != newVisibility)
            {
                if (newVisibility == TimerVisibilityMode.Always)
                {
                    RequestVisibility(true);
                }
                else if (newVisibility == TimerVisibilityMode.OnNotificationOnly && _state.Status == FocusTimerStatus.Working)
                {
                    RequestVisibility(false);
                }
            }
        }

        // Notify that settings have changed (for position and other UI updates)
        SettingsChanged?.Invoke(this, EventArgs.Empty);
    }

    #endregion Public Methods

    #region Private Methods

    private void OnTimerTick(object? sender, EventArgs args)
    {
        if (!_isRunning) return;

        var now = DateTime.Now;
        var workEnd = _scheduler.ParseTime(_settings.WorkEndTime);

        // Check if work day ended
        if (now.TimeOfDay >= workEnd.TimeOfDay)
        {
            if (_state.Status != FocusTimerStatus.DayEnded)
            {
                SetState(new DayEndedState());
            }
            return;
        }

        // Check if in lunch window
        if (_scheduler.IsInLunchWindow(now, _settings))
        {
            if (_state.Status != FocusTimerStatus.LunchMode)
            {
                SetState(new LunchModeState());
            }
            return;
        }

        _currentState.HandleTick(this, now);

        // Notify state change for UI updates
        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, _state.Status));
    }

    #endregion Private Methods

    public void SetState(IFocusTimerState newState)
    {
        var previousStatus = _state.Status;
        _currentState = newState;
        _state.Status = newState.Status;

        newState.OnEnter(this);

        if (newState.Status == FocusTimerStatus.NotificationTriggered)
        {
            // Trigger notification specific logic
            if (_settings.PlaySoundOnNotification)
            {
                _notificationService.PlayBreakSound();
            }
            RequestVisibility(true);
            BreakNotificationTriggered?.Invoke(this, EventArgs.Empty);
        }
        else if (newState.Status == FocusTimerStatus.Working && previousStatus == FocusTimerStatus.BreakActive)
        {
            // Trigger focus sound when break ends and focus starts
            if (_settings.PlaySoundOnNotification)
            {
                _notificationService.PlayFocusSound();
            }
        }

        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, previousStatus));
    }

    public void CompleteBreak()
    {
        CompleteBreakInternal(isEarly: false);
    }

    private void CompleteBreakInternal(bool isEarly)
    {
        var now = DateTime.Now;
        double minutesTaken;

        if (isEarly && _state.CurrentBreakStartTime.HasValue)
        {
            var breakTaken = now - _state.CurrentBreakStartTime.Value;
            minutesTaken = breakTaken.TotalMinutes;
        }
        else
        {
            minutesTaken = _state.CurrentBreakDurationMinutes;
        }

        _state.CurrentBreakPoolMinutes = Math.Max(0, _state.CurrentBreakPoolMinutes - minutesTaken);
        _state.BreakTimeTakenMinutes += minutesTaken;
        _state.RemainingBreakCount = Math.Max(0, _state.RemainingBreakCount - 1);
        _state.LastBreakEndTime = now;

        _state.CurrentBreakStartTime = null;
        _state.BreakTimeRemaining = null;

        SetState(new WorkingState());
        BreakEnded?.Invoke(this, EventArgs.Empty);

        // Update window visibility
        var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        if (visibilityMode == TimerVisibilityMode.OnNotificationOnly)
        {
            RequestVisibility(false);
        }

        _ = _stateManager.PersistStateAsync(_state);
    }

    public void RequestVisibility(bool show)
    {
        _notificationService.RequestWindowVisibility(show);
        WindowVisibilityRequested?.Invoke(this, show);
    }
}