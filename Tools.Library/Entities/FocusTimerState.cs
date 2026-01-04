namespace Tools.Library.Entities;

/// <summary>
/// Represents the current state of the Focus Timer.
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
/// Represents a scheduled break checkpoint.
/// </summary>
public class BreakCheckpoint
{
    /// <summary>
    /// Gets or sets the scheduled time for this break.
    /// </summary>
    public DateTime ScheduledTime { get; set; }

    /// <summary>
    /// Gets or sets the calculated duration for this break in minutes.
    /// </summary>
    public double DurationMinutes { get; set; }

    /// <summary>
    /// Gets or sets whether this break is in Zone A (morning) or Zone B (afternoon).
    /// </summary>
    public bool IsMorningZone { get; set; }

    /// <summary>
    /// Gets or sets whether this checkpoint has been completed.
    /// </summary>
    public bool IsCompleted { get; set; }

    /// <summary>
    /// Gets or sets whether this checkpoint was skipped.
    /// </summary>
    public bool IsSkipped { get; set; }
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
    /// Gets or sets the scheduled break checkpoints for the day.
    /// </summary>
    public List<BreakCheckpoint> ScheduledBreaks { get; set; } = [];

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
    /// Gets the break bank percentage (0-100) for fuel gauge display.
    /// </summary>
    public double BreakBankPercentage => TotalDailyBreakMinutes > 0
        ? Math.Min(100, (CurrentBreakPoolMinutes / TotalDailyBreakMinutes) * 100)
        : 0;

    /// <summary>
    /// Gets or sets the total daily break minutes for percentage calculation.
    /// </summary>
    public double TotalDailyBreakMinutes { get; set; }

    /// <summary>
    /// Gets a message describing the current state.
    /// </summary>
    public string StatusMessage => Status switch
    {
        FocusTimerStatus.Stopped => "Timer stopped",
        FocusTimerStatus.Working => TimeUntilNextBreak.HasValue
            ? $"Next break in {FormatTimeSpan(TimeUntilNextBreak.Value)}"
            : "Working...",
        FocusTimerStatus.NotificationTriggered => "Break time!",
        FocusTimerStatus.BreakActive => BreakTimeRemaining.HasValue
            ? $"Break ends in {FormatTimeSpan(BreakTimeRemaining.Value)}"
            : "On break",
        FocusTimerStatus.LunchMode => "Lunch break",
        FocusTimerStatus.DayEnded => "Work day ended",
        _ => "Unknown"
    };

    /// <summary>
    /// Gets the display text for the countdown timer.
    /// </summary>
    public string CountdownDisplay => Status switch
    {
        FocusTimerStatus.Working => TimeUntilNextBreak.HasValue
            ? FormatTimeSpan(TimeUntilNextBreak.Value)
            : "--:--",
        FocusTimerStatus.BreakActive => BreakTimeRemaining.HasValue
            ? FormatTimeSpan(BreakTimeRemaining.Value)
            : "--:--",
        FocusTimerStatus.NotificationTriggered => FormatMinutes(CurrentBreakDurationMinutes),
        _ => "--:--"
    };

    private static string FormatTimeSpan(TimeSpan ts)
    {
        if (ts.TotalHours >= 1)
            return $"{(int)ts.TotalHours}:{ts.Minutes:D2}:{ts.Seconds:D2}";
        return $"{ts.Minutes:D2}:{ts.Seconds:D2}";
    }

    private static string FormatMinutes(double minutes)
    {
        var ts = TimeSpan.FromMinutes(minutes);
        return FormatTimeSpan(ts);
    }
}
