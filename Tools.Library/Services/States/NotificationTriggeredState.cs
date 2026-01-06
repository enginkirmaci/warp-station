using Tools.Library.Entities;

namespace Tools.Library.Services.States;

public class NotificationTriggeredState : IFocusTimerState
{
    public FocusTimerStatus Status => FocusTimerStatus.NotificationTriggered;

    public void OnEnter(FocusTimerService context)
    {
        // Already handled by context.UpdateStatus which triggers events
        // But we handle UX side here if we want more decoupling
    }

    public void HandleTick(FocusTimerService context, DateTime now)
    {
        // Just waiting for user action
    }
}
