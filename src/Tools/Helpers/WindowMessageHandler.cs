#if WINDOWS
using System.Runtime.InteropServices;
using Tools.Library.Services.Abstractions;

namespace Tools.Helpers;

/// <summary>
/// Handles Windows message processing for hotkey support.
/// Implements Single Responsibility Principle - only responsible for window message handling.
/// </summary>
internal sealed class WindowMessageHandler
{
    #region Constants
    private const int WM_HOTKEY = 0x0312;
    private const int HOTKEY_ID = 9000;
    private const int GWL_WNDPROC = -4;
    #endregion

    #region Fields
    private readonly IClipboardPasswordService _clipboardPasswordService;
    private IntPtr _oldWndProc = IntPtr.Zero;
    private WndProcDelegate? _wndProcDelegate;
    #endregion

    #region Constructor
    /// <summary>
    /// Initializes a new instance of the WindowMessageHandler.
    /// </summary>
    /// <param name="clipboardPasswordService">Service for handling clipboard password operations.</param>
    public WindowMessageHandler(IClipboardPasswordService clipboardPasswordService)
    {
        _clipboardPasswordService = clipboardPasswordService;
    }
    #endregion

    #region Public Methods
    /// <summary>
    /// Installs the window message hook.
    /// </summary>
    /// <param name="hwnd">The window handle to hook.</param>
    public void Install(nint hwnd)
    {
        _wndProcDelegate = WndProc;
        var newProcPtr = Marshal.GetFunctionPointerForDelegate(_wndProcDelegate);
        _oldWndProc = SetWindowLongPtr(hwnd, GWL_WNDPROC, newProcPtr);
    }

    /// <summary>
    /// Uninstalls the window message hook.
    /// </summary>
    /// <param name="hwnd">The window handle to unhook.</param>
    public void Uninstall(nint hwnd)
    {
        if (_oldWndProc != IntPtr.Zero && hwnd != nint.Zero)
        {
            SetWindowLongPtr(hwnd, GWL_WNDPROC, _oldWndProc);
            _oldWndProc = IntPtr.Zero;
        }
    }
    #endregion

    #region Private Methods
    /// <summary>
    /// Window procedure for processing messages.
    /// </summary>
    private IntPtr WndProc(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam)
    {
        if (msg == WM_HOTKEY && wParam.ToInt32() == HOTKEY_ID)
        {
            _ = _clipboardPasswordService.HandleHotkeyAsync();
            return IntPtr.Zero;
        }
        return CallWindowProc(_oldWndProc, hWnd, msg, wParam, lParam);
    }
    #endregion

    #region PInvoke
    private delegate IntPtr WndProcDelegate(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongPtrW", SetLastError = true)]
    private static extern IntPtr SetWindowLongPtrW(IntPtr hWnd, int nIndex, IntPtr newProc);

    [DllImport("user32.dll", EntryPoint = "SetWindowLongW", SetLastError = true)]
    private static extern int SetWindowLongW(IntPtr hWnd, int nIndex, int newProc);

    private static IntPtr SetWindowLongPtr(nint hWnd, int nIndex, IntPtr newProc)
    {
        return IntPtr.Size == 8
            ? SetWindowLongPtrW(hWnd, nIndex, newProc)
            : new IntPtr(SetWindowLongW(hWnd, nIndex, newProc.ToInt32()));
    }

    [DllImport("user32.dll", EntryPoint = "CallWindowProcW", SetLastError = true)]
    private static extern IntPtr CallWindowProc(IntPtr lpPrevWndFunc, IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
    #endregion
}
#endif
