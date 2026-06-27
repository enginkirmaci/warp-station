using System.Diagnostics;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

public class SnapItService : ISnapItService, IDisposable
{
    private const string SnapItExeName = "Tools.SnapIt.exe";
    private const string SnapItProcessName = "Tools.SnapIt";

    private readonly string _snapItExePath;
    private System.Timers.Timer? _monitorTimer;
    private bool _isRunning;

    public bool IsRunning
    {
        get => _isRunning;
        private set
        {
            if (_isRunning == value) return;
            _isRunning = value;
            RunningChanged?.Invoke(this, value);
        }
    }

    public event EventHandler<bool>? RunningChanged;

    public SnapItService()
    {
        _snapItExePath = Path.Combine(AppContext.BaseDirectory, SnapItExeName);

        _monitorTimer = new System.Timers.Timer(2000) { AutoReset = true };
        _monitorTimer.Elapsed += (_, _) => RefreshStatus();
        _monitorTimer.Start();

        RefreshStatus();
    }

    public Task StartAsync()
    {
        if (IsRunning) return Task.CompletedTask;

        try
        {
            if (!File.Exists(_snapItExePath))
            {
                Log.Logger.Warning("SnapIt executable not found: {Path}", _snapItExePath);
                return Task.CompletedTask;
            }

            Process.Start(new ProcessStartInfo
            {
                FileName = _snapItExePath,
                UseShellExecute = false
            });

            IsRunning = true;
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to start SnapIt");
        }

        return Task.CompletedTask;
    }

    public void Stop()
    {
        try
        {
            var processes = Process.GetProcessesByName(SnapItProcessName);
            foreach (var p in processes)
            {
                p.Kill();
                p.Dispose();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to stop SnapIt");
        }

        IsRunning = false;
    }

    private void RefreshStatus()
    {
        try
        {
            var processes = Process.GetProcessesByName(SnapItProcessName);
            IsRunning = processes.Length > 0;
            foreach (var p in processes) p.Dispose();
        }
        catch
        {
            IsRunning = false;
        }
    }

    public void Dispose()
    {
        _monitorTimer?.Stop();
        _monitorTimer?.Dispose();
        _monitorTimer = null;
    }
}
