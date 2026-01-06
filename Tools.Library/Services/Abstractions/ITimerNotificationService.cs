namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Handles notifications and UX orchestration for the Focus Timer.
/// </summary>
public interface ITimerNotificationService
{
    /// <summary>
    /// Plays the break notification sound.
    /// </summary>
    void PlayBreakSound();

    /// <summary>
    /// Plays the focus notification sound.
    /// </summary>
    void PlayFocusSound();

    /// <summary>
    /// Requests a change in the timer window visibility.
    /// </summary>
    void RequestWindowVisibility(bool show);
}
