using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implements the scheduling logic for the Focus Timer.
/// </summary>
public class FocusTimerScheduler : IFocusTimerScheduler
{
    public void Recalculate(FocusTimerState state, FocusTimerSettings settings)
    {
        var now = DateTime.Now;
        var workStart = ParseTime(settings.WorkStartTime);
        var workEnd = ParseTime(settings.WorkEndTime);
        var lunchStart = ParseTime(settings.LunchStartTime);
        var lunchEnd = lunchStart.AddMinutes(settings.LunchDurationMinutes);

        // If we haven't taken any breaks today, the reference point is the work start time (or now if we started late)
        if (!state.LastBreakEndTime.HasValue)
        {
            state.LastBreakEndTime = now.TimeOfDay > workStart.TimeOfDay ? now : workStart;
        }

        if (state.RemainingBreakCount <= 0)
        {
            state.NextBreakTime = null;
            state.TimeUntilNextBreak = null;
            return;
        }

        // Calculate next break duration (simple average of remaining pool)
        state.CurrentBreakDurationMinutes = state.CurrentBreakPoolMinutes / state.RemainingBreakCount;

        // Handle snooze
        if (state.SnoozedUntil.HasValue && state.SnoozedUntil > now)
        {
            state.NextBreakTime = state.SnoozedUntil.Value;
            state.TimeUntilNextBreak = state.SnoozedUntil.Value - now;
            return;
        }

        state.SnoozedUntil = null;

        // Fixed-interval logic:
        // Remaining time = WorkEnd - LastBreakEndTime - Lunch (if applicable)
        double totalRemainingWorkMinutes = CalculateRemainingWorkMinutes(state.LastBreakEndTime.Value, workEnd, lunchStart, lunchEnd);

        if (totalRemainingWorkMinutes <= 0)
        {
            state.NextBreakTime = null;
            state.TimeUntilNextBreak = null;
            return;
        }

        // Interval is total remaining work time divided by remaining "working blocks" (remaining breaks + 1)
        double intervalMinutes = totalRemainingWorkMinutes / (state.RemainingBreakCount + 1);

        // Next break is LastBreakEndTime + interval
        var nextBreakTime = state.LastBreakEndTime.Value.AddMinutes(intervalMinutes);

        // If next break falls during lunch, push to after lunch
        if (IsInLunchWindow(nextBreakTime, settings))
        {
            nextBreakTime = DateTime.Today.Add(lunchEnd.TimeOfDay).AddMinutes(5); // 5 min buffer after lunch
        }

        state.NextBreakTime = nextBreakTime;
        state.TimeUntilNextBreak = nextBreakTime - now;
    }

    public bool IsInLunchWindow(DateTime time, FocusTimerSettings settings)
    {
        var lunchStart = ParseTime(settings.LunchStartTime);
        var lunchEnd = lunchStart.AddMinutes(settings.LunchDurationMinutes);

        return time.TimeOfDay >= lunchStart.TimeOfDay && time.TimeOfDay < lunchEnd.TimeOfDay;
    }

    public DateTime ParseTime(string timeString)
    {
        if (TimeSpan.TryParse(timeString, out var time))
        {
            return DateTime.Today.Add(time);
        }
        return DateTime.Today.AddHours(9); // Default to 9 AM
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
}
