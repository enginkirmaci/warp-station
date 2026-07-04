using Avalonia.Threading;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerProvider using Avalonia's DispatcherTimer.
/// </summary>
/// <remarks>
/// A single <see cref="DispatcherTimer"/> is created lazily and reused across
/// <see cref="Start"/> calls (only its <see cref="DispatcherTimer.Interval"/>
/// changes), avoiding per-start timer churn and the closure accumulation that
/// came from recreating the timer plus its Tick handler each time.
/// </remarks>
public class DispatcherTimerProvider : ITimerProvider, IDisposable
{
    private readonly EventHandler _tickHandler;
    private DispatcherTimer? _timer;
    private bool _disposed;

    public DispatcherTimerProvider()
    {
        // Single fixed handler — no fresh closure per Start() call.
        _tickHandler = (_, _) => Tick?.Invoke(this, EventArgs.Empty);
    }

    public event EventHandler? Tick;

    public bool IsEnabled => _timer?.IsEnabled ?? false;

    public void Start(TimeSpan interval)
    {
        if (_disposed)
        {
            return;
        }

        _timer ??= new DispatcherTimer();
        _timer.Tick -= _tickHandler; // no-op on first start; safe otherwise

        _timer.Stop();
        _timer.Interval = interval;
        _timer.Tick += _tickHandler;
        _timer.Start();
    }

    public void Stop()
    {
        _timer?.Stop();
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        if (_timer != null)
        {
            _timer.Stop();
            _timer.Tick -= _tickHandler;
        }

        _disposed = true;
    }
}
