using Tools.Library.Services.Abstractions;
using Tools.SnapIt.Contracts;

namespace Tools.Services;

/// <summary>
/// In-process adapter that exposes the SnapIt engine (<see cref="ISnapManager"/>) through the
/// application-level <see cref="ISnapItService"/> contract. Starts/stops snapping within the
/// Tools process instead of launching a separate executable.
/// </summary>
public class SnapItService : ISnapItService, IDisposable
{
    private readonly ISnapManager _snapManager;
    private bool _disposed;

    public SnapItService(ISnapManager snapManager)
    {
        _snapManager = snapManager;
        _snapManager.StatusChanged += OnStatusChanged;
    }

    public bool IsRunning => _snapManager.IsRunning;

    public event EventHandler<bool>? RunningChanged;

    public Task StartAsync()
    {
        if (IsRunning)
        {
            return Task.CompletedTask;
        }

        return _snapManager.InitializeAsync();
    }

    public void Stop()
    {
        if (IsRunning)
        {
            // SnapManager.Dispose tears down hooks/services and raises StatusChanged(false),
            // which propagates through RunningChanged.
            _snapManager.Dispose();
        }
    }

    private void OnStatusChanged(bool isRunning)
    {
        RunningChanged?.Invoke(this, isRunning);
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _snapManager.StatusChanged -= OnStatusChanged;
        _disposed = true;
    }
}
