using Tools.Library.Entities;

namespace Tools.Library.Services.States;

public class BreakActiveState : IFocusTimerState
{
    public FocusTimerStatus Status => FocusTimerStatus.BreakActive;

    public void OnEnter(FocusTimerService context)
    {
        // No special logic on enter
    }

    public void HandleTick(FocusTimerService context, DateTime now)
    {
        var state = context.CurrentState;
        if (!state.CurrentBreakStartTime.HasValue) return;

        var elapsed = now - state.CurrentBreakStartTime.Value;
        var totalBreakDuration = TimeSpan.FromMinutes(state.CurrentBreakDurationMinutes);
        var remaining = totalBreakDuration - elapsed;

        if (remaining <= TimeSpan.Zero)
        {
            // Break ended naturally
            context.CompleteBreak();
        }
        else
        {
            state.BreakTimeRemaining = remaining;
        }
    }
}
