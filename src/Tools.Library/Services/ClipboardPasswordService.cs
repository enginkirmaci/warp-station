using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Input.Platform;
using Avalonia.Threading;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

public class ClipboardPasswordService : IClipboardPasswordService
{
#if WINDOWS
    private const int HOTKEY_ID = 9000;
    private const uint MOD_CONTROL = 0x0002;
    private const uint MOD_SHIFT = 0x0004;
    private const uint VK_V = 0x56;
    private const uint CF_UNICODETEXT = 13;
    private const uint GMEM_MOVABLE = 0x0002;

    [DllImport("user32.dll")]
    private static extern bool RegisterHotKey(nint hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll")]
    private static extern bool UnregisterHotKey(nint hWnd, int id);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool OpenClipboard(nint hWndNewOwner);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool CloseClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool EmptyClipboard();

    [DllImport("user32.dll", SetLastError = true)]
    private static extern nint SetClipboardData(uint uFormat, nint hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern nint GlobalAlloc(uint uFlags, nint dwBytes);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern nint GlobalLock(nint hMem);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GlobalUnlock(nint hMem);

    private nint _hwnd;
    private bool _isRegistered;
#else
    private GlobalHotkeyManager? _globalHotkey;
#endif

    private readonly ISettingsService _settingsService;

    public ClipboardPasswordService(ISettingsService settingsService)
    {
        _settingsService = settingsService;
    }

    public async Task InitializeAsync()
    {
        // Load settings if needed
        await Task.CompletedTask;
    }

    public void RegisterHotKeys(nint hwnd)
    {
#if WINDOWS
        _hwnd = hwnd;

        // Register Ctrl+Shift+V
        _isRegistered = RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT, VK_V);

        if (!_isRegistered)
        {
            Log.Logger.Warning("Failed to register global hotkey Ctrl+Shift+V");
        }
#else
        // The manager picks the best backend for the session: the GlobalShortcuts portal
        // on Wayland (works over Wayland-native windows), X11 root-window grabs
        // otherwise/as fallback. Both own their platform connection; no handle needed.
        _globalHotkey = new GlobalHotkeyManager();
        _globalHotkey.HotkeyPressed += OnGlobalHotkeyPressed;
        _globalHotkey.TryRegister();
#endif
    }

    public void UnregisterHotKeys()
    {
#if WINDOWS
        if (_isRegistered && _hwnd != nint.Zero)
        {
            UnregisterHotKey(_hwnd, HOTKEY_ID);
            _isRegistered = false;
        }
#else
        if (_globalHotkey != null)
        {
            _globalHotkey.HotkeyPressed -= OnGlobalHotkeyPressed;
            _globalHotkey.Dispose();
            _globalHotkey = null;
        }
#endif
    }

    public async Task HandleHotkeyAsync()
    {
        string password = await GetDecryptedPasswordAsync();
        if (!string.IsNullOrEmpty(password))
        {
#if WINDOWS
            SetClipboardText(password);
#else
            await SetClipboardTextAsync(password);
#endif
        }
    }

#if !WINDOWS
    // The hotkey backend raises the press on its own thread; clipboard access is
    // marshalled to the UI thread where the Avalonia clipboard backend lives.
    private void OnGlobalHotkeyPressed()
    {
        Dispatcher.UIThread.Post(() => _ = HandleHotkeyAsync());
    }

    private static async Task SetClipboardTextAsync(string text)
    {
        var lifetime = Application.Current?.ApplicationLifetime as IClassicDesktopStyleApplicationLifetime;
        var clipboard = lifetime?.MainWindow is { } window ? TopLevel.GetTopLevel(window)?.Clipboard : null;
        if (clipboard is null)
        {
            Log.Logger.Warning("ClipboardPassword: no clipboard available to copy the password to");
            return;
        }

        await clipboard.SetTextAsync(text);
    }
#endif

#if WINDOWS
    private static void SetClipboardText(string text)
    {
        if (!OpenClipboard(nint.Zero))
            return;

        try
        {
            EmptyClipboard();

            int bytes = (text.Length + 1) * 2;
            var hGlobal = GlobalAlloc(GMEM_MOVABLE, (nint)bytes);
            if (hGlobal == nint.Zero)
                return;

            var ptr = GlobalLock(hGlobal);
            if (ptr != nint.Zero)
            {
                Marshal.Copy(text.ToCharArray(), 0, ptr, text.Length);
                GlobalUnlock(hGlobal);
                SetClipboardData(CF_UNICODETEXT, hGlobal);
            }
        }
        finally
        {
            CloseClipboard();
        }
    }
#endif

    private async Task<string> GetDecryptedPasswordAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        var encrypted = settings.ClipboardPassword?.EncryptedPassword;
        if (string.IsNullOrEmpty(encrypted))
            return string.Empty;
        try
        {
            return Encoding.UTF8.GetString(Convert.FromBase64String(encrypted));
        }
        catch
        {
            return string.Empty;
        }
    }
}