using Microsoft.UI.Dispatching;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerProvider using DispatcherQueueTimer.
/// </summary>
public class DispatcherTimerProvider : ITimerProvider
{
    private readonly DispatcherQueue _dispatcherQueue;
    private DispatcherQueueTimer? _timer;

    public event EventHandler? Tick;

    public bool IsEnabled => _timer?.IsRunning ?? false;

    public DispatcherTimerProvider(DispatcherQueue dispatcherQueue)
    {
        _dispatcherQueue = dispatcherQueue;
    }

    public void Start(TimeSpan interval)
    {
        if (_timer == null)
        {
            _timer = _dispatcherQueue.CreateTimer();
            _timer.Tick += (s, e) => Tick?.Invoke(this, EventArgs.Empty);
        }

        _timer.Interval = interval;
        _timer.Start();
    }

    public void Stop()
    {
        _timer?.Stop();
    }
}
