#if WINDOWS
using Microsoft.Win32;

namespace Tools.Helpers;

/// <summary>
/// Manages launching the app automatically when the user signs in.
///
/// Because this is an unpackaged (non-MSIX) app, OS startup is configured via the
/// per-user registry Run key. <c>settings.json</c> stays the single source of truth:
/// the app reconciles the registry to match the configured flag on every launch.
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
#endif
