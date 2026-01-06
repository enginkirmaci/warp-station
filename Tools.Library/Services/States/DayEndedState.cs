using Tools.Library.Entities;

namespace Tools.Library.Services.States;

public class DayEndedState : IFocusTimerState
{
    public FocusTimerStatus Status => FocusTimerStatus.DayEnded;

    public void OnEnter(FocusTimerService context)
    {
        context.RequestVisibility(true);
    }

    public void HandleTick(FocusTimerService context, DateTime now)
    {
        // Day has ended, nothing to do unless we support restarts
    }
}
