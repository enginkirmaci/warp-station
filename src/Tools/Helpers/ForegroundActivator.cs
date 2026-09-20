using System.Runtime.InteropServices;
using Avalonia.Controls;

namespace Tools.Helpers;

/// <summary>
/// Grants the main window keyboard focus at startup. A window whose launching
/// process was background (the boot-time supervisor) is denied foreground rights
/// by Windows, so a plain Activate() can land behind — the rejected call is
/// re-attempted through the foreground thread's input queue, and finally via a
/// minimize/restore round trip, which always completes with focus.
/// </summary>
internal static class ForegroundActivator
{
#if WINDOWS
    private const int SW_MINIMIZE = 6;
    private const int SW_RESTORE = 9;

    public static void EnsureForeground(Window window)
    {
        var handle = window.TryGetPlatformHandle()?.Handle ?? nint.Zero;
        if (handle == nint.Zero)
        {
            return;
        }

        var foreground = GetForegroundWindow();
        if (foreground == handle)
        {
            return;
        }

        var foregroundThread = GetWindowThreadProcessId(foreground, out _);
        var currentThread = GetCurrentThreadId();
        var attached = foregroundThread != 0
            && foregroundThread != currentThread
            && AttachThreadInput(foregroundThread, currentThread, true);
        try
        {
            if (SetForegroundWindow(handle))
            {
                return;
            }
        }
        finally
        {
            if (attached)
            {
                AttachThreadInput(foregroundThread, currentThread, false);
            }
        }

        ShowWindow(handle, SW_MINIMIZE);
        ShowWindow(handle, SW_RESTORE);
        SetForegroundWindow(handle);
    }

    [DllImport("user32.dll")]
    private static extern nint GetForegroundWindow();

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(nint hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool SetForegroundWindow(nint hWnd);

    [DllImport("user32.dll")]
    private static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);

    [DllImport("kernel32.dll")]
    private static extern uint GetCurrentThreadId();

    [DllImport("user32.dll")]
    private static extern bool ShowWindow(nint hWnd, int nCmdShow);
#else
    public static void EnsureForeground(Window window)
    {
    }
#endif
}
