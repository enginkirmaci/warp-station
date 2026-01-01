using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Tools.Library.Services.Abstractions;
using Windows.ApplicationModel.DataTransfer;

namespace Tools.Services;

public class ClipboardPasswordService : IClipboardPasswordService
{
    private const int HOTKEY_ID = 9000;
    private const uint MOD_CONTROL = 0x0002;
    private const uint MOD_SHIFT = 0x0004;
    private const uint VK_V = 0x56;

    private readonly ISettingsService _settingsService;
    private nint _hwnd;
    private bool _isRegistered;

    [DllImport("user32.dll")]
    private static extern bool RegisterHotKey(nint hWnd, int id, uint fsModifiers, uint vk);

    [DllImport("user32.dll")]
    private static extern bool UnregisterHotKey(nint hWnd, int id);

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
        _hwnd = hwnd;

        // Register Ctrl+Shift+V
        _isRegistered = RegisterHotKey(hwnd, HOTKEY_ID, MOD_CONTROL | MOD_SHIFT, VK_V);

        if (!_isRegistered)
        {
            Debug.WriteLine("Failed to register global hotkey Ctrl+Shift+V");
        }
    }

    public void UnregisterHotKeys()
    {
        if (_isRegistered && _hwnd != nint.Zero)
        {
            UnregisterHotKey(_hwnd, HOTKEY_ID);
            _isRegistered = false;
        }
    }

    public async Task HandleHotkeyAsync()
    {
        string password = await GetDecryptedPasswordAsync();
        if (!string.IsNullOrEmpty(password))
        {
            var dataPackage = new DataPackage();
            dataPackage.SetText(password);
            Clipboard.SetContent(dataPackage);
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