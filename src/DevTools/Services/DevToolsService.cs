using System.Diagnostics;
using Microsoft.Extensions.Hosting;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace DevTools.Services;

public class DevToolsService : IDisposable
{
    private readonly NamedPipeServer _pipeServer;
    private readonly IProcessLauncher _processLauncher;
    private readonly IHostApplicationLifetime _lifetime;
    private readonly string _toolsExePath;
    private Process? _toolsProcess;
    private bool _isRunning;

    public DevToolsService(NamedPipeServer pipeServer, IProcessLauncher processLauncher, IHostApplicationLifetime lifetime)
    {
        _pipeServer = pipeServer;
        _processLauncher = processLauncher;
        _lifetime = lifetime;

        // Resolve Tools.exe relative to DevTools.exe. Candidates, in priority order:
        //   1. ./bin/Tools.exe       — production split layout (Tools ships in a bin/ subfolder)
        //   2. ../Tools.exe          — parent dir (development when both exes share bin/)
        //   3. ./Tools.exe           — same dir (development when co-located)
        var currentDir = AppContext.BaseDirectory;
        _toolsExePath = new[]
        {
            Path.Combine(currentDir, "bin", "Tools.exe"),
            Path.Combine(currentDir, "..", "Tools.exe"),
            Path.Combine(currentDir, "Tools.exe"),
        }.First(File.Exists);
    }

    public async Task StartAsync()
    {
        if (_isRunning)
            return;

        _isRunning = true;
        Log.Information("[DevToolsService] Starting DevTools service");

        // Start Tools.exe
        await StartToolsAsync();

        // Start Named Pipe Server
        await _pipeServer.StartAsync();

        Log.Information("[DevToolsService] DevTools service started successfully");
    }

    public void Stop()
    {
        _isRunning = false;

        // Stop Tools.exe
        if (_toolsProcess != null && !_toolsProcess.HasExited)
        {
            try
            {
                _toolsProcess.Kill();
                Log.Information("[DevToolsService] Tools.exe terminated");
            }
            catch (Exception ex)
            {
                Log.Error(ex, "[DevToolsService] Failed to terminate Tools.exe");
            }
        }

        // Stop pipe server
        _pipeServer.Stop();

        Log.Information("[DevToolsService] DevTools service stopped");
    }

    private Task StartToolsAsync()
    {
        if (!File.Exists(_toolsExePath))
        {
            Log.Warning("[DevToolsService] Tools.exe not found at {Path}, skipping auto-start", _toolsExePath);
            return Task.CompletedTask;
        }

        try
        {
            Log.Information("[DevToolsService] Starting Tools.exe from {Path}", _toolsExePath);

            _toolsProcess = Process.Start(new ProcessStartInfo
            {
                FileName = _toolsExePath,
                UseShellExecute = true,
                Verb = "runas" // Request admin elevation
            });

            if (_toolsProcess != null)
            {
                Log.Information("[DevToolsService] Tools.exe started with PID {PID}", _toolsProcess.Id);

                // Monitor Tools.exe process
                _ = Task.Run(() => MonitorToolsProcessAsync());
            }
        }
        catch (Exception ex)
        {
            Log.Error(ex, "[DevToolsService] Failed to start Tools.exe");
        }

        return Task.CompletedTask;
    }

    private async Task MonitorToolsProcessAsync()
    {
        try
        {
            await _toolsProcess!.WaitForExitAsync();
            Log.Information("[DevToolsService] Tools.exe exited with code {ExitCode}, shutting down DevTools", _toolsProcess.ExitCode);

            // Stop DevTools when Tools.exe exits
            Stop();
            _lifetime.StopApplication();
        }
        catch (Exception ex)
        {
            Log.Error(ex, "[DevToolsService] Error monitoring Tools.exe");
        }
    }

    public void Dispose()
    {
        Stop();
    }
}