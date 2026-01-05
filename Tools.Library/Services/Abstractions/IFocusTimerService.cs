using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Event arguments for Focus Timer state changes.
/// </summary>
public class FocusTimerStateChangedEventArgs : EventArgs
{
    /// <summary>
    /// Gets the current state of the timer.
    /// </summary>
    public FocusTimerState State { get; }

    /// <summary>
    /// Gets the previous status before the change.
    /// </summary>
    public FocusTimerStatus PreviousStatus { get; }

    public FocusTimerStateChangedEventArgs(FocusTimerState state, FocusTimerStatus previousStatus)
    {
        State = state;
        PreviousStatus = previousStatus;
    }
}

/// <summary>
/// Provides Focus Timer functionality with dynamic break scheduling.
/// </summary>
public interface IFocusTimerService
{
    /// <summary>
    /// Occurs when the timer state changes.
    /// </summary>
    event EventHandler<FocusTimerStateChangedEventArgs>? StateChanged;

    /// <summary>
    /// Occurs when a break notification should be shown.
    /// </summary>
    event EventHandler? BreakNotificationTriggered;

    /// <summary>
    /// Occurs when a break starts.
    /// </summary>
    event EventHandler? BreakStarted;

    /// <summary>
    /// Occurs when a break ends.
    /// </summary>
    event EventHandler? BreakEnded;

    /// <summary>
    /// Occurs when timer requests window visibility change.
    /// </summary>
    event EventHandler<bool>? WindowVisibilityRequested;

    /// <summary>
    /// Gets the current state of the timer.
    /// </summary>
    FocusTimerState CurrentState { get; }

    /// <summary>
    /// Gets the current settings.
    /// </summary>
    FocusTimerSettings Settings { get; }

    /// <summary>
    /// Gets whether the timer is currently running.
    /// </summary>
    bool IsRunning { get; }

    /// <summary>
    /// Starts the Focus Timer.
    /// </summary>
    Task StartAsync();

    /// <summary>
    /// Stops the Focus Timer.
    /// </summary>
    Task StopAsync();

    /// <summary>
    /// User accepts the break notification and starts the break.
    /// </summary>
    void TakeBreak();

    /// <summary>
    /// User manually starts a break during working time.
    /// </summary>
    void TakeBreakNow();

    /// <summary>
    /// User snoozes the break notification.
    /// </summary>
    /// <param name="minutes">Number of minutes to snooze.</param>
    void SnoozeBreak(int minutes = 5);

    /// <summary>
    /// User ends the break early - unused time returns to pool.
    /// </summary>
    void EndBreakEarly();

    /// <summary>
    /// Recalculates the break schedule based on current state.
    /// </summary>
    void RecalculateSchedule();

    /// <summary>
    /// Loads settings and initializes the service.
    /// </summary>
    Task InitializeAsync();

    /// <summary>
    /// Updates the settings and recalculates if running.
    /// </summary>
    Task UpdateSettingsAsync(FocusTimerSettings settings);
}
