using Tools.Library.Entities;

namespace Tools.Library.Services.States;

public class WorkingState : IFocusTimerState
{
    public FocusTimerStatus Status => FocusTimerStatus.Working;

    public void OnEnter(FocusTimerService context)
    {
        context.RecalculateSchedule();
    }

    public void HandleTick(FocusTimerService context, DateTime now)
    {
        var state = context.CurrentState;

        // Check for snooze
        if (state.SnoozedUntil.HasValue)
        {
            if (now >= state.SnoozedUntil.Value)
            {
                state.SnoozedUntil = null;
                context.SetState(new NotificationTriggeredState());
            }
            else
            {
                state.TimeUntilNextBreak = state.SnoozedUntil.Value - now;
            }
            return;
        }

        // Update time until next break
        if (state.NextBreakTime.HasValue)
        {
            state.TimeUntilNextBreak = state.NextBreakTime.Value - now;

            // Check if it's break time
            if (now >= state.NextBreakTime.Value)
            {
                context.SetState(new NotificationTriggeredState());
            }
        }
        else
        {
            // No more breaks scheduled or need recalculation
            context.RecalculateSchedule();
        }
    }
}
