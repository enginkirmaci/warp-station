#if WINDOWS
using Microsoft.Win32;
#endif
using Serilog;

namespace Tools.Library.Helpers;

/// <summary>
/// Manages the OS launch-at-sign-in registration for the DevTools pair. Windows uses
/// the per-user registry Run key (value "DevTools"); other platforms use a freedesktop
/// autostart desktop entry in <c>~/.config/autostart</c> (devtools.desktop).
/// settings.json stays the single source of truth (General.StartAtBoot, no GUI
/// surface): the supervisor and the Tools GUI both reconcile on launch, and every
/// writer registers the same <see cref="ResolveBootTarget"/> executable, so the entry
/// never forks into two competing registrations.
/// </summary>
public static class AutoStartHelper
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";

    private const string ValueName = "DevTools";

    private const string DesktopEntryFile = "devtools.desktop";

    /// <summary>
    /// Resolves the executable the sign-in registration should launch: the DevTools
    /// supervisor when one sits next to the current executable (it launches Tools and,
    /// on Windows, hosts the pipe that launches children non-elevated), otherwise the
    /// current executable (standalone or AppImage layout). Inside an AppImage the
    /// registration targets the AppImage file itself (the mounted binary path is
    /// ephemeral) with the extract-and-run flag so it also boots on hosts without FUSE.
    /// The AppImage env variables are only honored when the process actually runs from
    /// inside that image (its binary sits under APPDIR) — they otherwise leak into
    /// child processes of unrelated AppImage-hosted apps and would register the wrong
    /// target.
    /// </summary>
    /// <returns>The executable path plus any launch arguments, quoted-ready.</returns>
    public static string ResolveBootTarget()
    {
        var current = Environment.ProcessPath;

        // APPDIR is the AppImage runtime's mount/extract dir and always contains the
        // running binary in a genuine AppImage launch.
        var appDir = Environment.GetEnvironmentVariable("APPDIR");
        var appImagePath = Environment.GetEnvironmentVariable("APPIMAGE");
        if (!string.IsNullOrEmpty(current)
            && !string.IsNullOrEmpty(appDir)
            && !string.IsNullOrEmpty(appImagePath)
            && current.StartsWith(appDir, StringComparison.Ordinal)
            && File.Exists(appImagePath))
        {
            return appImagePath + " --appimage-extract-and-run";
        }

        if (string.IsNullOrEmpty(current))
        {
            return string.Empty;
        }

        var exeName = OperatingSystem.IsWindows() ? "DevTools.exe" : "DevTools";
        var currentDir = Path.GetDirectoryName(current);
        if (currentDir is null)
        {
            return current;
        }

        // The mirror of DevToolsService's Tools candidates: the supervisor sits in the
        // parent dir (production split layout, Tools ships in a bin/ subfolder) or next
        // to Tools (development, both exes share the output dir).
        var supervisor = new[]
            {
                Path.Combine(currentDir, "..", exeName),
                Path.Combine(currentDir, exeName)
            }
            .FirstOrDefault(File.Exists);

        // Normalize so a persisted registration never carries a ".." segment.
        return supervisor is null ? current : Path.GetFullPath(supervisor);
    }

    /// <summary>
    /// Reconciles the registration with the configured flag. When enabled, registers
    /// <paramref name="targetPath"/> to launch at sign-in; when disabled, removes the
    /// registration (a missing registration is a successful no-op).
    /// </summary>
    /// <param name="enabled">Whether the app should start at sign-in.</param>
    /// <param name="targetPath">Executable (with any launch arguments) to register.</param>
    /// <returns><see langword="false"/> only when the OS write failed.</returns>
    public static bool Sync(bool enabled, string targetPath)
    {
        if (enabled && string.IsNullOrWhiteSpace(targetPath))
        {
            Log.Logger.Warning("Launch-at-sign-in registration skipped: no executable path resolved");
            return false;
        }

        try
        {
            if (enabled)
            {
                Enable(targetPath);
            }
            else
            {
                Disable();
            }

            return true;
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Failed to sync the launch-at-sign-in registration (enabled: {Enabled})", enabled);
            return false;
        }
    }

#if WINDOWS
    private static void Enable(string targetPath)
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true);
        key.SetValue(ValueName, targetPath, RegistryValueKind.String);
    }

    private static void Disable()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
        key?.DeleteValue(ValueName, throwOnMissingValue: false);
    }
#else
    private static string AutostartDirectory
        => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "autostart");

    // SpecialFolder.ApplicationData maps to ~/.config on Linux, the XDG autostart home.
    private static string DesktopEntryPath => Path.Combine(AutostartDirectory, DesktopEntryFile);

    private static void Enable(string targetPath)
    {
        // A nonexistent XDG_CONFIG_HOME makes GetFolderPath return empty; writing
        // anyway would land in a CWD-relative "autostart" folder.
        if (string.IsNullOrWhiteSpace(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData)))
        {
            throw new InvalidOperationException("The XDG config home could not be resolved");
        }

        Directory.CreateDirectory(AutostartDirectory);
        File.WriteAllText(DesktopEntryPath,
            $"""
             [Desktop Entry]
             Type=Application
             Name=DevTools
             Comment=DevTools launch-at-sign-in entry
             Exec="{targetPath}"
             Terminal=false
             X-GNOME-Autostart-enabled=true
             Categories=Development;
             """);
    }

    private static void Disable()
    {
        if (File.Exists(DesktopEntryPath))
        {
            File.Delete(DesktopEntryPath);
        }
    }
#endif
}
