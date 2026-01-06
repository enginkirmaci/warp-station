using Tools.Library.Entities;

namespace Tools.Library.Services.States;

/// <summary>
/// Defines the contract for a Focus Timer state.
/// </summary>
public interface IFocusTimerState
{
    /// <summary>
    /// Gets the status represented by this state.
    /// </summary>
    FocusTimerStatus Status { get; }

    /// <summary>
    /// Called when entering the state.
    /// </summary>
    void OnEnter(FocusTimerService context);

    /// <summary>
    /// Handles a timer tick in this state.
    /// </summary>
    void HandleTick(FocusTimerService context, DateTime now);
}
