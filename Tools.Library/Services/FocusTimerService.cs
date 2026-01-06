using System.Diagnostics;
using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

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
    private readonly DispatcherQueue _dispatcherQueue;
    private DispatcherQueueTimer? _timer;
    private FocusTimerSettings _settings = new();
    private FocusTimerState _state = new();
    private bool _isRunning;

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

    #endregion Properties

    #region Constructor

    public FocusTimerService(
        ISettingsService settingsService, 
        IFocusTimerScheduler scheduler,
        IFocusTimerStateManager stateManager,
        DispatcherQueue dispatcherQueue)
    {
        _settingsService = settingsService;
        _scheduler = scheduler;
        _stateManager = stateManager;
        _dispatcherQueue = dispatcherQueue;
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

        RecalculateSchedule();
        UpdateStatus(FocusTimerStatus.Working);

        // Request window visibility based on settings
        var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        Debug.WriteLine($"[FocusTimerService] StartAsync: Requesting window visibility. Mode={visibilityMode}, ShouldShow={visibilityMode == TimerVisibilityMode.Always}");
        WindowVisibilityRequested?.Invoke(this, visibilityMode == TimerVisibilityMode.Always);

        Debug.WriteLine($"[FocusTimerService] StartAsync: Starting timer. NextBreakTime={_state.NextBreakTime:HH:mm}, TimeUntilNextBreak={_state.TimeUntilNextBreak?.TotalMinutes:F1}min");
        StartTimer();

        // Fire initial state change to update UI with calculated schedule
        Debug.WriteLine($"[FocusTimerService] StartAsync: Firing initial StateChanged event. Status={_state.Status}");
        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, FocusTimerStatus.Stopped));

        await _stateManager.PersistStateAsync(_state);
    }

    public async Task StopAsync()
    {
        if (!_isRunning) return;

        _isRunning = false;
        StopTimer();
        UpdateStatus(FocusTimerStatus.Stopped);
        WindowVisibilityRequested?.Invoke(this, false);
        await _stateManager.PersistStateAsync(_state);
    }

    public void TakeBreak()
    {
        if (_state.Status != FocusTimerStatus.NotificationTriggered) return;

        _state.CurrentBreakStartTime = DateTime.Now;
        _state.BreakTimeRemaining = TimeSpan.FromMinutes(_state.CurrentBreakDurationMinutes);
        UpdateStatus(FocusTimerStatus.BreakActive);
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
        UpdateStatus(FocusTimerStatus.BreakActive);
        BreakStarted?.Invoke(this, EventArgs.Empty);
    }

    public void SnoozeBreak(int minutes = 5)
    {
        if (_state.Status != FocusTimerStatus.NotificationTriggered) return;

        _state.SnoozedUntil = DateTime.Now.AddMinutes(minutes);
        UpdateStatus(FocusTimerStatus.Working);

        // Keep window visible during snooze if in "always" mode
        var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        if (visibilityMode == TimerVisibilityMode.OnNotificationOnly)
        {
            WindowVisibilityRequested?.Invoke(this, false);
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
            RecalculateSchedule();

            // Handle visibility change if it was modified
            if (oldVisibility != newVisibility)
            {
                if (newVisibility == TimerVisibilityMode.Always)
                {
                    WindowVisibilityRequested?.Invoke(this, true);
                }
                else if (newVisibility == TimerVisibilityMode.OnNotificationOnly && _state.Status == FocusTimerStatus.Working)
                {
                    WindowVisibilityRequested?.Invoke(this, false);
                }
            }
        }

        // Notify that settings have changed (for position and other UI updates)
        SettingsChanged?.Invoke(this, EventArgs.Empty);
    }

    #endregion Public Methods

    #region Private Methods

    private void StartTimer()
    {
        if (_timer != null) return;

        _timer = _dispatcherQueue.CreateTimer();
        _timer.Interval = TimeSpan.FromMilliseconds(TimerIntervalMs);
        _timer.Tick += OnTimerTick;
        _timer.Start();
    }

    private void StopTimer()
    {
        if (_timer == null) return;

        _timer.Stop();
        _timer.Tick -= OnTimerTick;
        _timer = null;
    }

    private void OnTimerTick(DispatcherQueueTimer sender, object args)
    {
        if (!_isRunning) return;

        var now = DateTime.Now;
        var workEnd = _scheduler.ParseTime(_settings.WorkEndTime);

        // Check if work day ended
        if (now.TimeOfDay >= workEnd.TimeOfDay)
        {
            HandleDayEnd();
            return;
        }

        // Check if in lunch window
        if (_scheduler.IsInLunchWindow(now, _settings))
        {
            if (_state.Status != FocusTimerStatus.LunchMode)
            {
                UpdateStatus(FocusTimerStatus.LunchMode);
            }
            return;
        }

        // Transition out of lunch mode
        if (_state.Status == FocusTimerStatus.LunchMode)
        {
            UpdateStatus(FocusTimerStatus.Working);
            RecalculateSchedule();
        }

        switch (_state.Status)
        {
            case FocusTimerStatus.Working:
                HandleWorkingTick(now);
                break;

            case FocusTimerStatus.BreakActive:
                HandleBreakTick(now);
                break;
        }

        // Notify state change for UI updates
        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, _state.Status));
    }

    private void HandleWorkingTick(DateTime now)
    {
        // Check for snooze
        if (_state.SnoozedUntil.HasValue)
        {
            if (now >= _state.SnoozedUntil.Value)
            {
                _state.SnoozedUntil = null;
                TriggerBreakNotification();
            }
            else
            {
                _state.TimeUntilNextBreak = _state.SnoozedUntil.Value - now;
            }
            return;
        }

        // Update time until next break
        if (_state.NextBreakTime.HasValue)
        {
            _state.TimeUntilNextBreak = _state.NextBreakTime.Value - now;

            // Check if it's break time
            if (now >= _state.NextBreakTime.Value)
            {
                TriggerBreakNotification();
            }
        }
        else
        {
            // No more breaks scheduled or need recalculation
            RecalculateSchedule();
        }
    }

    private void HandleBreakTick(DateTime now)
    {
        if (!_state.CurrentBreakStartTime.HasValue) return;

        var elapsed = now - _state.CurrentBreakStartTime.Value;
        var totalBreakDuration = TimeSpan.FromMinutes(_state.CurrentBreakDurationMinutes);
        var remaining = totalBreakDuration - elapsed;

        if (remaining <= TimeSpan.Zero)
        {
            // Break ended naturally
            CompleteBreak();
        }
        else
        {
            _state.BreakTimeRemaining = remaining;
        }
    }

    private void TriggerBreakNotification()
    {
        UpdateStatus(FocusTimerStatus.NotificationTriggered);

        // Play sound if enabled
        if (_settings.PlaySoundOnNotification)
        {
            PlayNotificationSound();
        }

        // Request window to show and come to front
        WindowVisibilityRequested?.Invoke(this, true);
        BreakNotificationTriggered?.Invoke(this, EventArgs.Empty);
    }

    private void CompleteBreak()
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

        RecalculateSchedule();
        UpdateStatus(FocusTimerStatus.Working);
        BreakEnded?.Invoke(this, EventArgs.Empty);

        // Update window visibility
        var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        if (visibilityMode == TimerVisibilityMode.OnNotificationOnly)
        {
            WindowVisibilityRequested?.Invoke(this, false);
        }

        _ = _stateManager.PersistStateAsync(_state);
    }

    private void HandleDayEnd()
    {
        if (_state.Status != FocusTimerStatus.DayEnded)
        {
            UpdateStatus(FocusTimerStatus.DayEnded);
            WindowVisibilityRequested?.Invoke(this, true);
        }
    }

    private void UpdateStatus(FocusTimerStatus newStatus)
    {
        var previousStatus = _state.Status;
        _state.Status = newStatus;
        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, previousStatus));
    }

    private void PlayNotificationSound()
    {
        try
        {
            // Use Windows MessageBeep API for notification sound
            MessageBeep(0x00000040); // MB_ICONINFORMATION
        }
        catch
        {
            // Ignore sound errors
        }
    }

    [DllImport("user32.dll")]
    private static extern bool MessageBeep(uint uType);

    #endregion Private Methods
}