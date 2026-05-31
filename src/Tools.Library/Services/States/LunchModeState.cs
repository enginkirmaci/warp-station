using Tools.Library.Entities;

namespace Tools.Library.Services.States;

public class LunchModeState : IFocusTimerState
{
    public FocusTimerStatus Status => FocusTimerStatus.LunchMode;

    public void OnEnter(FocusTimerService context)
    {
        // No special logic on enter
    }

    public void HandleTick(FocusTimerService context, DateTime now)
    {
        // Check if out of lunch window
        if (!context.Scheduler.IsInLunchWindow(now, context.Settings))
        {
            context.SetState(new WorkingState());
        }
    }
}
