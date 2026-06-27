#if WINDOWS
using System.Diagnostics;
using System.Security.Principal;

namespace Tools.Helpers;

/// <summary>
/// Helpers for launching external processes, including de-elevation
/// (launching a non-elevated child from an elevated parent).
/// </summary>
public static class ProcessHelper
{
    /// <summary>
    /// Returns true when the current process is running with administrator
    /// (elevated) privileges.
    /// </summary>
    public static bool IsRunningElevated()
    {
        try
        {
            using var identity = WindowsIdentity.GetCurrent();
            return new WindowsPrincipal(identity).IsInRole(WindowsBuiltInRole.Administrator);
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// Launches a process as the standard (non-elevated) desktop user.
    ///
    /// When the current process is elevated, child processes inherit elevation
    /// by default. To run e.g. VS Code non-elevated, the launch is routed through
    /// the interactive desktop Explorer instance (explorer.exe), which runs as the
    /// non-elevated user — so the spawned process inherits its token.
    /// </summary>
    /// <returns>True if the process was started; otherwise false.</returns>
    public static bool StartAsDesktopUser(string fileName, string? arguments = null)
    {
        if (string.IsNullOrWhiteSpace(fileName)) return false;

        // Nothing to de-elevate when we're already running non-elevated.
        if (!IsRunningElevated())
        {
            return StartNormal(fileName, arguments);
        }

        try
        {
            // ShellWindows binds to the running desktop Explorer, which runs
            // unelevated. Launching through it inherits Explorer's non-elevated token.
            var shellWindowsType = Type.GetTypeFromCLSID(
                new Guid("9BA05972-F6A8-11CF-A442-00A0C90A8F39"));

            if (shellWindowsType == null)
                return StartNormal(fileName, arguments);

            dynamic shellWindows = Activator.CreateInstance(shellWindowsType)!;

            dynamic? shellApplication = null;
            for (int i = 0; i < shellWindows.Count; i++)
            {
                dynamic? window = shellWindows.Item(i);
                if (window == null) continue;

                shellApplication = window.Document?.Application;
                if (shellApplication != null) break;
            }

            if (shellApplication == null)
                return StartNormal(fileName, arguments);

            // IShellDispatch.ShellExecute(File, vArgs, vDir, vOperation, vShow)
            shellApplication.ShellExecute(fileName, arguments ?? string.Empty, string.Empty, "open", 1);
            return true;
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[ProcessHelper] StartAsDesktopUser failed, falling back to normal launch: {ex.Message}");
            return StartNormal(fileName, arguments);
        }
    }

    private static bool StartNormal(string fileName, string? arguments)
    {
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = fileName,
                Arguments = arguments ?? string.Empty,
                UseShellExecute = true
            });
            return true;
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[ProcessHelper] Failed to start process '{fileName}': {ex.Message}");
            return false;
        }
    }
}
#endif
