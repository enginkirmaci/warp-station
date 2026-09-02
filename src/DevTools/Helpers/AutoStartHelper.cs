#if WINDOWS
using Microsoft.Win32;

namespace DevTools.Helpers;

/// <summary>
/// Manages launching <c>DevTools.exe</c> automatically when the user signs in.
///
/// Because this is an unpackaged (non-MSIX) app, OS startup is configured via the
/// per-user registry Run key. <c>settings.json</c> stays the single source of truth:
/// the supervisor reconciles the registry to match the configured flag on every launch.
/// DevTools.exe in turn launches <c>Tools.exe</c>, so both start together at sign-in.
/// </summary>
public static class AutoStartHelper
{
    private const string RunKeyPath = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string ValueName = "DevTools";

    /// <summary>
    /// Reconciles the registry with the configured flag. When enabled, registers the
    /// current executable to launch at sign-in; when disabled, removes the registration.
    /// </summary>
    /// <param name="enabled">Whether the app should start at boot.</param>
    public static void Sync(bool enabled)
    {
        try
        {
            if (enabled)
            {
                Enable();
            }
            else
            {
                Disable();
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[AutoStartHelper] Failed to sync startup registration: {ex.Message}");
        }
    }

    /// <summary>
    /// Returns true only when the Run key contains a value for this app that points
    /// at the current executable, so the result reflects the real OS state.
    /// </summary>
    public static bool IsEnabled()
    {
        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath);
            if (key?.GetValue(ValueName) is not string value)
            {
                return false;
            }

            return Normalize(value).Equals(Normalize(CurrentExePath), StringComparison.OrdinalIgnoreCase);
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[AutoStartHelper] Failed to read startup registration: {ex.Message}");
            return false;
        }
    }

    private static void Enable()
    {
        using var key = Registry.CurrentUser.CreateSubKey(RunKeyPath, writable: true);
        key.SetValue(ValueName, CurrentExePath, RegistryValueKind.String);
    }

    private static void Disable()
    {
        using var key = Registry.CurrentUser.OpenSubKey(RunKeyPath, writable: true);
        key?.DeleteValue(ValueName, throwOnMissingValue: false);
    }

    private static string CurrentExePath => "\"" + Environment.ProcessPath + "\"";

    // Strips surrounding quotes so a stored "C:\app\app.exe" compares equal to C:\app\app.exe.
    private static string Normalize(string path) => path.Trim().Trim('"');
}
#else
namespace DevTools.Helpers;

/// <summary>
/// Manages launching the DevTools supervisor automatically when the user signs in
/// (Linux). Uses a freedesktop autostart desktop entry in
/// <c>~/.config/autostart</c>; <c>settings.json</c> stays the single source of truth:
/// the supervisor reconciles the entry to match the configured flag on every launch.
/// DevTools in turn launches Tools, so both start together at sign-in.
/// </summary>
public static class AutoStartHelper
{
    private static string AutostartDirectory
        => Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData), "autostart");

    // SpecialFolder.ApplicationData maps to ~/.config on Linux, the XDG autostart home.
    private static string DesktopEntryPath => Path.Combine(AutostartDirectory, "devtools.desktop");

    private static string ExecValue => "\"" + Environment.ProcessPath + "\"";

    /// <summary>
    /// Reconciles the autostart entry with the configured flag. When enabled, writes the
    /// desktop entry for the current executable; when disabled, removes it.
    /// </summary>
    /// <param name="enabled">Whether the app should start at boot.</param>
    public static void Sync(bool enabled)
    {
        try
        {
            if (enabled)
            {
                Enable();
            }
            else
            {
                Disable();
            }
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[AutoStartHelper] Failed to sync startup registration: {ex.Message}");
        }
    }

    /// <summary>
    /// Returns true only when the autostart entry exists and its Exec value points
    /// at the current executable, so the result reflects the real OS state.
    /// </summary>
    public static bool IsEnabled()
    {
        try
        {
            if (!File.Exists(DesktopEntryPath))
            {
                return false;
            }

            var execLine = File.ReadAllLines(DesktopEntryPath)
                .FirstOrDefault(line => line.StartsWith("Exec=", StringComparison.Ordinal));

            return execLine != null
                && Normalize(execLine["Exec=".Length..]).Equals(Normalize(ExecValue), StringComparison.Ordinal);
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[AutoStartHelper] Failed to read startup registration: {ex.Message}");
            return false;
        }
    }

    private static void Enable()
    {
        Directory.CreateDirectory(AutostartDirectory);
        File.WriteAllText(DesktopEntryPath,
            $"""
             [Desktop Entry]
             Type=Application
             Name=DevTools
             Comment=DevTools supervisor — launches Tools at sign-in
             Exec={ExecValue}
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

    private static string Normalize(string path) => path.Trim().Trim('"');
}
#endif
