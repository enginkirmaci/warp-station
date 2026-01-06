using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides logic for calculating the Focus Timer schedule.
/// </summary>
public interface IFocusTimerScheduler
{
    /// <summary>
    /// Recalculates the next break time based on the current state and settings.
    /// </summary>
    /// <param name="state">The current timer state.</param>
    /// <param name="settings">The timer settings.</param>
    void Recalculate(FocusTimerState state, FocusTimerSettings settings);

    /// <summary>
    /// Checks if the given time is within the lunch window.
    /// </summary>
    bool IsInLunchWindow(DateTime time, FocusTimerSettings settings);

    /// <summary>
    /// Parses a time string into a DateTime object for today.
    /// </summary>
    DateTime ParseTime(string timeString);
}
