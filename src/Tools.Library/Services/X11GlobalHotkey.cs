#if !WINDOWS
using System.Runtime.InteropServices;
using Serilog;

namespace Tools.Library.Services;

/// <summary>
/// Registers a global hotkey (Ctrl+Shift+V) through an X11 passive grab (XGrabKey on the
/// root window of a dedicated display connection) — the same approach xbindkeys uses.
/// The grabbed key presses are delivered to that connection only, so a background thread
/// drains the event queue and raises <see cref="HotkeyPressed"/>.
/// Best-effort by nature: on X11 sessions it works globally; under Wayland it only sees
/// keys typed into XWayland clients; without libX11 it cannot register at all.
/// </summary>
internal sealed class X11GlobalHotkey : IDisposable
{
    private const int KeyPressEventType = 2;
    private const uint ShiftMask = 1;
    private const uint LockMask = 1 << 1;   // Caps Lock
    private const uint ControlMask = 1 << 2;
    private const uint Mod2Mask = 1 << 4;   // Num Lock
    private const int GrabModeAsync = 0;
    private const ulong XkV = 0x76;         // XK_v
    private const int XEventSize = 192;     // sizeof(XEvent) on 64-bit

    // X11 modifier matching is exact, so the grab is repeated for the Caps Lock /
    // Num Lock combinations, exactly like xbindkeys does.
    private static readonly uint[] IgnoredModifierCombinations =
    {
        0,
        LockMask,
        Mod2Mask,
        LockMask | Mod2Mask
    };

    private readonly XErrorHandler _errorHandler;
    private nint _display;
    private nint _rootWindow;
    private uint _keycode;
    private Thread? _eventThread;
    private volatile bool _grabFailed;
    private volatile bool _stopRequested;
    private bool _registered;

    /// <summary>Raised on the hotkey thread for each Ctrl+Shift+V press.</summary>
    public event Action? HotkeyPressed;

    public X11GlobalHotkey()
    {
        // Keep a rooted delegate so libX11's callback target cannot be collected.
        _errorHandler = OnXError;
    }

    /// <summary>
    /// Opens the X11 connection and installs the grabs. Returns false when there is no
    /// X11 display, or the key is already grabbed by another program.
    /// </summary>
    public bool TryRegister()
    {
        try
        {
            _display = XOpenDisplay(null);
            if (_display == nint.Zero)
            {
                Log.Logger.Warning("X11 global hotkey: could not open the X display (Wayland session without XWayland?)");
                return false;
            }

            // The error handler is thread-local and this thread also drives Avalonia's
            // own X11 connection, so install it only for the grab setup and restore the
            // previous handler as soon as the grabs are flushed.
            var previousHandler = XSetErrorHandler(_errorHandler);

            _rootWindow = XRootWindowOfScreen(XDefaultScreenOfDisplay(_display));
            _keycode = XKeysymToKeycode(_display, XkV);
            if (_keycode == 0)
            {
                Log.Logger.Warning("X11 global hotkey: no keycode found for 'v' in the current keyboard mapping");
                return false;
            }

            foreach (var ignore in IgnoredModifierCombinations)
            {
                XGrabKey(_display, _keycode, ControlMask | ShiftMask | ignore, _rootWindow, false, GrabModeAsync, GrabModeAsync);
            }

            // XGrabKey reports BadAccess asynchronously through the error handler; flush
            // the request queue so a failed grab surfaces before we claim success.
            XSync(_display, false);
            XSetErrorHandler(previousHandler);
            if (_grabFailed)
            {
                Log.Logger.Warning("X11 global hotkey: Ctrl+Shift+V is already grabbed by another program");
                return false;
            }

            _stopRequested = false;
            _registered = true;
            _eventThread = new Thread(EventLoop) { IsBackground = true, Name = "X11GlobalHotkey" };
            _eventThread.Start();
            return true;
        }
        catch (Exception ex) // DllNotFoundException when libX11 is absent, etc.
        {
            Log.Logger.Warning(ex, "X11 global hotkey could not be registered");
            Dispose();
            return false;
        }
    }

    public void Dispose()
    {
        _stopRequested = true;

        if (_display == nint.Zero)
        {
            return;
        }

        if (_registered && _keycode != 0)
        {
            foreach (var ignore in IgnoredModifierCombinations)
            {
                XUngrabKey(_display, _keycode, ControlMask | ShiftMask | ignore, _rootWindow);
            }
        }

        // The event loop polls with a short sleep (it never blocks inside XNextEvent),
        // so it observes the stop flag quickly; close the display only once it has exited.
        _eventThread?.Join(TimeSpan.FromSeconds(1));
        XCloseDisplay(_display);
        _display = nint.Zero;
        _eventThread = null;
    }

    private int OnXError(nint display, nint errorEvent)
    {
        _grabFailed = true;
        return 0;
    }

    private void EventLoop()
    {
        var xEvent = Marshal.AllocHGlobal(XEventSize);
        try
        {
            while (!_stopRequested)
            {
                if (XPending(_display) == 0)
                {
                    Thread.Sleep(50);
                    continue;
                }

                XNextEvent(_display, xEvent);
                if (Marshal.ReadInt32(xEvent) != KeyPressEventType)
                {
                    continue;
                }

                if (Marshal.PtrToStructure<XKeyEvent>(xEvent).Keycode == _keycode)
                {
                    HotkeyPressed?.Invoke();
                }
            }
        }
        finally
        {
            Marshal.FreeHGlobal(xEvent);
        }
    }

    private delegate int XErrorHandler(nint display, nint errorEvent);

    // Layout mirrors the C XKeyEvent (an XEvent union member) on 64-bit.
    [StructLayout(LayoutKind.Sequential)]
    private struct XKeyEvent
    {
        public int Type;
        public nuint Serial;
        public int SendEvent;
        public nint Display;
        public nint Window;
        public nint Root;
        public nint Subwindow;
        public nuint Time;
        public int X;
        public int Y;
        public int XRoot;
        public int YRoot;
        public uint State;
        public uint Keycode;
        public int SameScreen;
    }

    [DllImport("libX11.so.1")]
    private static extern nint XOpenDisplay(string? display);

    [DllImport("libX11.so.1")]
    private static extern int XCloseDisplay(nint display);

    [DllImport("libX11.so.1")]
    private static extern nint XDefaultScreenOfDisplay(nint display);

    [DllImport("libX11.so.1")]
    private static extern nint XRootWindowOfScreen(nint screen);

    [DllImport("libX11.so.1")]
    private static extern uint XKeysymToKeycode(nint display, ulong keysym);

    [DllImport("libX11.so.1")]
    private static extern int XGrabKey(nint display, uint keycode, uint modifiers, nint grabWindow, bool ownerEvents, int pointerMode, int keyboardMode);

    [DllImport("libX11.so.1")]
    private static extern int XUngrabKey(nint display, uint keycode, uint modifiers, nint grabWindow);

    [DllImport("libX11.so.1")]
    private static extern int XNextEvent(nint display, nint xEvent);

    [DllImport("libX11.so.1")]
    private static extern int XPending(nint display);

    [DllImport("libX11.so.1")]
    private static extern int XSync(nint display, bool discard);

    [DllImport("libX11.so.1")]
    private static extern nint XSetErrorHandler(XErrorHandler handler);

    // Overload used to restore a previous handler captured as a raw function pointer.
    [DllImport("libX11.so.1")]
    private static extern void XSetErrorHandler(nint handler);
}
#endif
