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

    #endregion Events

    #region Properties

    public FocusTimerState CurrentState => _state;
    public FocusTimerSettings Settings => _settings;
    public bool IsRunning => _isRunning;

    #endregion Properties

    #region Constructor

    public FocusTimerService(ISettingsService settingsService, DispatcherQueue dispatcherQueue)
    {
        _settingsService = settingsService;
        _dispatcherQueue = dispatcherQueue;
    }

    #endregion Constructor

    #region Public Methods

    public async Task InitializeAsync()
    {
        var appSettings = await _settingsService.GetSettingsAsync();
        _settings = appSettings.FocusTimer ?? new FocusTimerSettings();

        // Check for daily reset
        CheckAndResetDailyState();
    }

    public async Task StartAsync()
    {
        if (_isRunning) return;

        await InitializeAsync();

        _isRunning = true;
        _state.TotalDailyBreakMinutes = _settings.TotalDailyBreakMinutes;

        // Initialize break pool if starting fresh
        if (_state.CurrentBreakPoolMinutes <= 0)
        {
            _state.CurrentBreakPoolMinutes = _settings.TotalDailyBreakMinutes;
            _state.RemainingBreakCount = _settings.DesiredBreakCount;
        }

        RecalculateSchedule();
        UpdateStatus(FocusTimerStatus.Working);

        // Request window visibility based on settings
        var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        System.Diagnostics.Debug.WriteLine($"[FocusTimerService] StartAsync: Requesting window visibility. Mode={visibilityMode}, ShouldShow={visibilityMode == TimerVisibilityMode.Always}");
        WindowVisibilityRequested?.Invoke(this, visibilityMode == TimerVisibilityMode.Always);

        System.Diagnostics.Debug.WriteLine($"[FocusTimerService] StartAsync: Starting timer. NextBreakTime={_state.NextBreakTime:HH:mm}, TimeUntilNextBreak={_state.TimeUntilNextBreak?.TotalMinutes:F1}min");
        StartTimer();

        // Fire initial state change to update UI with calculated schedule
        System.Diagnostics.Debug.WriteLine($"[FocusTimerService] StartAsync: Firing initial StateChanged event. Status={_state.Status}");
        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, FocusTimerStatus.Stopped));

        await PersistStateAsync();
    }

    public async Task StopAsync()
    {
        if (!_isRunning) return;

        _isRunning = false;
        StopTimer();
        UpdateStatus(FocusTimerStatus.Stopped);
        WindowVisibilityRequested?.Invoke(this, false);
        await PersistStateAsync();
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

        var breakTaken = DateTime.Now - _state.CurrentBreakStartTime;
        var minutesTaken = breakTaken?.TotalMinutes ?? 0;

        // Deduct actual time taken from pool
        _state.CurrentBreakPoolMinutes = Math.Max(0, _state.CurrentBreakPoolMinutes - minutesTaken);
        _state.BreakTimeTakenMinutes += minutesTaken;
        _state.RemainingBreakCount = Math.Max(0, _state.RemainingBreakCount - 1);
        _state.LastBreakEndTime = DateTime.Now;


        _state.CurrentBreakStartTime = null;
        _state.BreakTimeRemaining = null;

        RecalculateSchedule();
        UpdateStatus(FocusTimerStatus.Working);
        BreakEnded?.Invoke(this, EventArgs.Empty);

        // Update window visibility
        //var visibilityMode = (TimerVisibilityMode)_settings.TimerVisibilityMode;
        //if (visibilityMode == TimerVisibilityMode.OnNotificationOnly)
        //{
        //    WindowVisibilityRequested?.Invoke(this, false);
        //}

        _ = PersistStateAsync();
    }

    public void RecalculateSchedule()
    {
        var now = DateTime.Now;
        var workStart = ParseTime(_settings.WorkStartTime);
        var workEnd = ParseTime(_settings.WorkEndTime);
        var lunchStart = ParseTime(_settings.LunchStartTime);
        var lunchEnd = lunchStart.AddMinutes(_settings.LunchDurationMinutes);

        // If we haven't taken any breaks today, the reference point is the work start time (or now if we started late)
        if (!_state.LastBreakEndTime.HasValue)
        {
            _state.LastBreakEndTime = now.TimeOfDay > workStart.TimeOfDay ? now : workStart;
        }

        if (_state.RemainingBreakCount <= 0)
        {
            _state.NextBreakTime = null;
            _state.TimeUntilNextBreak = null;
            return;
        }

        // Calculate next break duration (simple average of remaining pool)
        _state.CurrentBreakDurationMinutes = _state.CurrentBreakPoolMinutes / _state.RemainingBreakCount;

        // Handle snooze
        if (_state.SnoozedUntil.HasValue && _state.SnoozedUntil > now)
        {
            _state.NextBreakTime = _state.SnoozedUntil.Value;
            _state.TimeUntilNextBreak = _state.SnoozedUntil.Value - now;
            return;
        }

        _state.SnoozedUntil = null;

        // Fixed-interval logic:
        // Remaining time = WorkEnd - LastBreakEndTime - Lunch (if applicable)
        double totalRemainingWorkMinutes = CalculateRemainingWorkMinutes(_state.LastBreakEndTime.Value, workEnd, lunchStart, lunchEnd);
        
        if (totalRemainingWorkMinutes <= 0)
        {
            _state.NextBreakTime = null;
            _state.TimeUntilNextBreak = null;
            return;
        }

        // Interval is total remaining work time divided by remaining "working blocks" (remaining breaks + 1)
        double intervalMinutes = totalRemainingWorkMinutes / (_state.RemainingBreakCount + 1);
        
        // Next break is LastBreakEndTime + interval
        var nextBreakTime = _state.LastBreakEndTime.Value.AddMinutes(intervalMinutes);

        // If next break falls during lunch, push to after lunch
        if (IsInLunchWindow(nextBreakTime))
        {
            nextBreakTime = DateTime.Today.Add(lunchEnd.TimeOfDay).AddMinutes(5); // 5 min buffer after lunch
        }

        _state.NextBreakTime = nextBreakTime;
        _state.TimeUntilNextBreak = nextBreakTime - now;
    }

    public async Task UpdateSettingsAsync(FocusTimerSettings settings)
    {
        _settings = settings;

        var appSettings = await _settingsService.GetSettingsAsync();
        appSettings.FocusTimer = settings;
        await _settingsService.SaveSettingsAsync(appSettings);

        if (_isRunning)
        {
            RecalculateSchedule();
        }
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
        var workEnd = ParseTime(_settings.WorkEndTime);

        // Check if work day ended
        if (now.TimeOfDay >= workEnd.TimeOfDay)
        {
            HandleDayEnd();
            return;
        }

        // Check if in lunch window
        if (IsInLunchWindow(now))
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
        var now = DateTime.Now;
        _state.CurrentBreakPoolMinutes = Math.Max(0, _state.CurrentBreakPoolMinutes - _state.CurrentBreakDurationMinutes);
        _state.BreakTimeTakenMinutes += _state.CurrentBreakDurationMinutes;
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

        _ = PersistStateAsync();
    }

    private void HandleDayEnd()
    {
        if (_state.Status != FocusTimerStatus.DayEnded)
        {
            UpdateStatus(FocusTimerStatus.DayEnded);
            WindowVisibilityRequested?.Invoke(this, true);
        }
    }

    private void CheckAndResetDailyState()
    {
        var today = DateTime.Today.ToString("yyyy-MM-dd");
        var persistedState = _settings.PersistedState;

        if (persistedState == null || persistedState.LastResetDate != today)
        {
            // Reset for new day
            _state = new FocusTimerState
            {
                CurrentBreakPoolMinutes = _settings.TotalDailyBreakMinutes,
                RemainingBreakCount = _settings.DesiredBreakCount,
                TotalDailyBreakMinutes = _settings.TotalDailyBreakMinutes,
                BreakTimeTakenMinutes = 0,
                LastBreakEndTime = null
            };
        }
        else
        {
            // Restore persisted state
            _state = new FocusTimerState
            {
                CurrentBreakPoolMinutes = persistedState.CurrentBreakPoolMinutes,
                RemainingBreakCount = persistedState.RemainingBreakCount,
                TotalDailyBreakMinutes = _settings.TotalDailyBreakMinutes,
                BreakTimeTakenMinutes = persistedState.BreakTimeTakenMinutes,
                LastBreakEndTime = DateTime.Now // Approximate for continuation
            };
        }
    }

    private async Task PersistStateAsync()
    {
        var appSettings = await _settingsService.GetSettingsAsync();
        if (appSettings.FocusTimer == null)
        {
            appSettings.FocusTimer = new FocusTimerSettings();
        }

        appSettings.FocusTimer.PersistedState = new FocusTimerPersistedState
        {
            LastResetDate = DateTime.Today.ToString("yyyy-MM-dd"),
            CurrentBreakPoolMinutes = _state.CurrentBreakPoolMinutes,
            RemainingBreakCount = _state.RemainingBreakCount,
            BreakTimeTakenMinutes = _state.BreakTimeTakenMinutes
        };

        await _settingsService.SaveSettingsAsync(appSettings);
    }

    private void UpdateStatus(FocusTimerStatus newStatus)
    {
        var previousStatus = _state.Status;
        _state.Status = newStatus;
        StateChanged?.Invoke(this, new FocusTimerStateChangedEventArgs(_state, previousStatus));
    }

    private DateTime ParseTime(string timeString)
    {
        if (TimeSpan.TryParse(timeString, out var time))
        {
            return DateTime.Today.Add(time);
        }
        return DateTime.Today.AddHours(9); // Default to 9 AM
    }

    private bool IsInLunchWindow(DateTime time)
    {
        var lunchStart = ParseTime(_settings.LunchStartTime);
        var lunchEnd = lunchStart.AddMinutes(_settings.LunchDurationMinutes);

        return time.TimeOfDay >= lunchStart.TimeOfDay && time.TimeOfDay < lunchEnd.TimeOfDay;
    }

    private double CalculateRemainingWorkMinutes(DateTime fromTime, DateTime workEnd, DateTime lunchStart, DateTime lunchEnd)
    {
        double remaining = 0;

        if (fromTime.TimeOfDay < lunchStart.TimeOfDay)
        {
            // Add time until lunch
            remaining += (lunchStart.TimeOfDay - fromTime.TimeOfDay).TotalMinutes;
            // Add time after lunch until work end
            remaining += (workEnd.TimeOfDay - lunchEnd.TimeOfDay).TotalMinutes;
        }
        else if (fromTime.TimeOfDay >= lunchEnd.TimeOfDay)
        {
            // Only time after lunch
            remaining = (workEnd.TimeOfDay - fromTime.TimeOfDay).TotalMinutes;
        }
        else
        {
            // During lunch - only time after lunch
            remaining = (workEnd.TimeOfDay - lunchEnd.TimeOfDay).TotalMinutes;
        }

        return Math.Max(0, remaining);
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
