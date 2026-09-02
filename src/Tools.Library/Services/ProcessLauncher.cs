using System.Diagnostics;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

public class ProcessLauncher : IProcessLauncher
{
    // Set by Electron hosts (VS Code & forks, some IDE terminals) that Tools may have
    // been launched from; it leaks into every child and turns Electron-packaged CLIs
    // (e.g. an AppImage of zcode) into bare Node processes.
    private const string ElectronRunAsNodeVariable = "ELECTRON_RUN_AS_NODE";

    public void StartProcess(string fileName, string? arguments = null, bool hidden = false, bool stripElectronEnvironment = false)
    {
        if (string.IsNullOrWhiteSpace(fileName))
            return;

        try
        {
            var startInfo = new ProcessStartInfo
            {
                FileName = fileName,
                Arguments = arguments ?? string.Empty,

                // Customizing the child environment requires UseShellExecute=false; the
                // exec-style spawn is fine here because stripping is only requested for
                // resolved executables (absolute paths or PATH-resolvable names).
                UseShellExecute = !stripElectronEnvironment,
                CreateNoWindow = hidden,
                WindowStyle = hidden ? ProcessWindowStyle.Hidden : ProcessWindowStyle.Normal
            };

            if (stripElectronEnvironment)
            {
                startInfo.EnvironmentVariables.Remove(ElectronRunAsNodeVariable);
            }

            // Process.Start returns an IDisposable wrapper around an OS handle.
            // With UseShellExecute=true the launched process runs independently of
            // this wrapper, so dispose immediately to release the kernel handle.
            using var process = Process.Start(startInfo);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to start process '{FileName}'", fileName);
        }
    }

    public Task StartProcessAsync(string fileName, string? arguments = null, bool hidden = false, bool stripElectronEnvironment = false)
    {
        StartProcess(fileName, arguments, hidden, stripElectronEnvironment);
        return Task.CompletedTask;
    }
}