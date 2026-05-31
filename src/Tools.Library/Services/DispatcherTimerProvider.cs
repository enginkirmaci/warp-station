using Avalonia.Threading;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerProvider using Avalonia's DispatcherTimer.
/// </summary>
public class DispatcherTimerProvider : ITimerProvider
{
    private DispatcherTimer? _timer;

    public event EventHandler? Tick;

    public bool IsEnabled => _timer?.IsEnabled ?? false;

    public void Start(TimeSpan interval)
    {
        _timer?.Stop();

        _timer = new DispatcherTimer
        {
            Interval = interval
        };
        _timer.Tick += (s, e) => Tick?.Invoke(this, EventArgs.Empty);
        _timer.Start();
    }

    public void Stop()
    {
        _timer?.Stop();
    }
}
