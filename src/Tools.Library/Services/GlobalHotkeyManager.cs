#if !WINDOWS
using Serilog;

namespace Tools.Library.Services;

/// <summary>
/// Registers the global password hotkey with the best backend the session offers:
/// on Wayland the GlobalShortcuts portal (compositor-level binding — works over
/// Wayland-native windows; the user confirms once in the compositor dialog), and X11
/// root-window grabs otherwise or as fallback (on a Wayland session without a portal
/// backend those only see presses typed into XWayland clients).
/// </summary>
internal sealed class GlobalHotkeyManager : IDisposable
{
    private GlobalShortcutsPortalHotkey? _portal;
    private X11GlobalHotkey? _x11;
    private bool _x11Attempted;
    private volatile bool _disposed;

    /// <summary>Raised on a backend thread for each hotkey press.</summary>
    public event Action? HotkeyPressed;

    public void TryRegister()
    {
        if (LinuxSessionInfo.IsWaylandSession)
        {
            _ = RegisterPortalAsync();
            return;
        }

        RegisterX11();
    }

    private async Task RegisterPortalAsync()
    {
        var portal = new GlobalShortcutsPortalHotkey();
        portal.HotkeyPressed += OnBackendPressed;
        bool registered;
        try
        {
            registered = await portal.TryRegisterAsync();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Global hotkey: GlobalShortcuts portal registration failed");
            registered = false;
        }

        if (_disposed)
        {
            portal.Dispose();
            return;
        }

        if (registered)
        {
            Log.Logger.Information("Global hotkey: Ctrl+Shift+V registered through the GlobalShortcuts portal");
            _portal = portal;
            return;
        }

        portal.Dispose();
        Log.Logger.Warning("Global hotkey: GlobalShortcuts portal unavailable or binding refused; falling back to X11 grabs");
        RegisterX11();
    }

    private void RegisterX11()
    {
        if (_x11Attempted)
        {
            return;
        }

        _x11Attempted = true;
        var x11 = new X11GlobalHotkey();
        x11.HotkeyPressed += OnBackendPressed;
        if (!x11.TryRegister())
        {
            x11.Dispose();
            Log.Logger.Warning("Failed to register global hotkey Ctrl+Shift+V");
            return;
        }

        _x11 = x11;
    }

    private void OnBackendPressed() => HotkeyPressed?.Invoke();

    public void Dispose()
    {
        _disposed = true;
        _portal?.Dispose();
        _portal = null;
        _x11?.Dispose();
        _x11 = null;
    }
}
#endif
