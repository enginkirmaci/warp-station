namespace Tools.Library.Entities;

/// <summary>
/// Represents the current status of the Focus Timer.
/// </summary>
public enum FocusTimerStatus
{
    /// <summary>Timer is stopped/not running.</summary>
    Stopped,

    /// <summary>Timer is running, user is working.</summary>
    Working,

    /// <summary>Break notification has been triggered, waiting for user response.</summary>
    NotificationTriggered,

    /// <summary>User is currently on a break.</summary>
    BreakActive,

    /// <summary>Currently in lunch window.</summary>
    LunchMode,

    /// <summary>Work day has ended.</summary>
    DayEnded
}

/// <summary>
/// Represents the visibility mode of the timer window.
/// </summary>
public enum TimerVisibilityMode
{
    /// <summary>Timer window is always visible.</summary>
    Always = 0,

    /// <summary>Timer window only shows on break notification.</summary>
    OnNotificationOnly = 1
}

/// <summary>
/// Represents the corner position of the timer window.
/// </summary>
public enum WindowCornerPosition
{
    /// <summary>Bottom right corner of the screen.</summary>
    BottomRight = 0,

    /// <summary>Bottom left corner of the screen.</summary>
    BottomLeft = 1,

    /// <summary>Top right corner of the screen.</summary>
    TopRight = 2,

    /// <summary>Top left corner of the screen.</summary>
    TopLeft = 3
}

/// <summary>
/// Runtime state for the Focus Timer service.
/// </summary>
public class FocusTimerState
{
    /// <summary>
    /// Gets or sets the current status of the timer.
    /// </summary>
    public FocusTimerStatus Status { get; set; } = FocusTimerStatus.Stopped;

    /// <summary>
    /// Gets or sets the current break pool remaining in minutes.
    /// </summary>
    public double CurrentBreakPoolMinutes { get; set; }

    /// <summary>
    /// Gets or sets the remaining break count for today.
    /// </summary>
    public int RemainingBreakCount { get; set; }

    /// <summary>
    /// Gets or sets the next scheduled break time.
    /// </summary>
    public DateTime? NextBreakTime { get; set; }

    /// <summary>
    /// Gets or sets the duration of the current/next break in minutes.
    /// </summary>
    public double CurrentBreakDurationMinutes { get; set; }

    /// <summary>
    /// Gets or sets the time remaining in the current break.
    /// </summary>
    public TimeSpan? BreakTimeRemaining { get; set; }

    /// <summary>
    /// Gets or sets the time until the next break.
    /// </summary>
    public TimeSpan? TimeUntilNextBreak { get; set; }

    /// <summary>
    /// Gets or sets the break that was snoozed (if any).
    /// </summary>
    public DateTime? SnoozedUntil { get; set; }

    /// <summary>
    /// Gets or sets the total break time taken today in minutes.
    /// </summary>
    public double BreakTimeTakenMinutes { get; set; }

    /// <summary>
    /// Gets or sets when the current break started.
    /// </summary>
    public DateTime? CurrentBreakStartTime { get; set; }

    /// <summary>
    /// Gets or sets the end time of the last completed break.
    /// Used as the starting point for the next work interval.
    /// </summary>
    public DateTime? LastBreakEndTime { get; set; }

    /// <summary>
    /// Gets or sets the total daily break minutes for percentage calculation.
    /// </summary>
    public double TotalDailyBreakMinutes { get; set; }
}
