namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides an abstraction for a system timer.
/// </summary>
public interface ITimerProvider
{
    /// <summary>
    /// Occurs when the timer interval has elapsed.
    /// </summary>
    event EventHandler? Tick;

    /// <summary>
    /// Starts the timer with the specified interval.
    /// </summary>
    void Start(TimeSpan interval);

    /// <summary>
    /// Stops the timer.
    /// </summary>
    void Stop();

    /// <summary>
    /// Gets whether the timer is currently enabled.
    /// </summary>
    bool IsEnabled { get; }
}
