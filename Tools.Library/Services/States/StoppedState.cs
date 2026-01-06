using Tools.Library.Entities;

namespace Tools.Library.Services.States;

public class StoppedState : IFocusTimerState
{
    public FocusTimerStatus Status => FocusTimerStatus.Stopped;

    public void OnEnter(FocusTimerService context)
    {
        // No special logic on enter
    }

    public void HandleTick(FocusTimerService context, DateTime now)
    {
        // Timer shouldn't tick in stopped state, but if it does, do nothing
    }
}
