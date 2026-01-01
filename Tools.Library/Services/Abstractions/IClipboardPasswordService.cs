namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides clipboard password generation and management.
/// </summary>
public interface IClipboardPasswordService
{
    /// <summary>
    /// Initializes the clipboard password service.
    /// </summary>
    Task InitializeAsync();

    /// <summary>
    /// Registers global hotkeys.
    /// </summary>
    void RegisterHotKeys(nint hwnd);

    /// <summary>
    /// Unregisters global hotkeys.
    /// </summary>
    void UnregisterHotKeys();

    /// <summary>
    /// Handles hotkey press events.
    /// </summary>
    Task HandleHotkeyAsync();
}
