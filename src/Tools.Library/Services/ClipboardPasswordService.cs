using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Input.Platform;
using Tools.Library.Services.Abstractions;

namespace Tools.Services;

public class ClipboardPasswordService : IClipboardPasswordService
{
#if WINDOWS
    private const int HOTKEY_ID = 9000;
    private const uint MOD_CONTROL = 0x0002;
    private const uint MOD_SHIFT = 0x0004;
    private const uint VK_V = 0x56;

    [DllImport("user32.dll")]
    private static extern bool RegisterHotKey(nint hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll")]
    private static extern bool UnregisterHotKey(nint hWnd, int id);

    private nint _hwnd;
    private bool _isRegistered;
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
            Debug.WriteLine("Failed to register global hotkey Ctrl+Shift+V");
        }
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
#endif
    }

    public async Task HandleHotkeyAsync()
    {
        string password = await GetDecryptedPasswordAsync();
        if (!string.IsNullOrEmpty(password))
        {
            if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime lifetime)
            {
                var clipboard = lifetime.MainWindow?.Clipboard;
                if (clipboard != null)
                {
                    await clipboard.SetTextAsync(password);
                }
            }
        }
    }

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