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

        // Resolve the Tools binary relative to DevTools. Candidates, in priority order:
        //   1. ./bin/Tools[.exe]  — production split layout (Tools ships in a bin/ subfolder)
        //   2. ../Tools[.exe]     — parent dir (development when both exes share bin/)
        //   3. ./Tools[.exe]      — same dir (development when co-located)
        var toolsFileName = OperatingSystem.IsWindows() ? "Tools.exe" : "Tools";
        var currentDir = AppContext.BaseDirectory;
        var candidates = new[]
        {
            Path.Combine(currentDir, "bin", toolsFileName),
            Path.Combine(currentDir, "..", toolsFileName),
            Path.Combine(currentDir, toolsFileName),
        };
        _toolsExePath = candidates.FirstOrDefault(File.Exists) ?? candidates[^1];
    }

    public async Task StartAsync()
    {
        if (_isRunning)
            return;

        _isRunning = true;
        Log.Information("[DevToolsService] Starting DevTools service");

        // Start Tools (the GUI) directly — the launcher behaviour on every platform.
        await StartToolsAsync();

        // The pipe only has a client on Windows: there Tools runs elevated and routes
        // process launches through it so children start non-elevated. The Tools client
        // is a stub on other platforms, so opening the pipe would serve nobody.
        if (OperatingSystem.IsWindows())
        {
            await _pipeServer.StartAsync();
        }

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
                // Windows: Tools runs elevated and launches children non-elevated through
                // the pipe. Other platforms just run Tools as the current user.
                Verb = OperatingSystem.IsWindows() ? "runas" : null
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