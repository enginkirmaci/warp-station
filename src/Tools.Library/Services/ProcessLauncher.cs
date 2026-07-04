using System.Diagnostics;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

public class ProcessLauncher : IProcessLauncher
{
    public void StartProcess(string fileName, string? arguments = null, bool hidden = false)
    {
        if (string.IsNullOrWhiteSpace(fileName))
            return;

        try
        {
            var startInfo = new ProcessStartInfo
            {
                FileName = fileName,
                Arguments = arguments ?? string.Empty,
                UseShellExecute = true,
                CreateNoWindow = hidden,
                WindowStyle = hidden ? ProcessWindowStyle.Hidden : ProcessWindowStyle.Normal
            };

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

    public Task StartProcessAsync(string fileName, string? arguments = null, bool hidden = false)
    {
        StartProcess(fileName, arguments, hidden);
        return Task.CompletedTask;
    }
}