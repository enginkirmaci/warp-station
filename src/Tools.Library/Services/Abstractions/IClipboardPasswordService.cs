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

    /// <summary>
    /// Base64-encodes the password and persists it to settings, in the exact format the
    /// hotkey path decodes.
    /// </summary>
    /// <param name="password">The plain-text password to store.</param>
    Task SetPasswordAsync(string password);
}
