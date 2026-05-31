namespace Tools.Helpers;

/// <summary>
/// Manages InfoBar display and auto-close functionality.
/// Simplified for cross-platform compatibility.
/// </summary>
public sealed class InfoBarManager
{
    private Action<string, string, string>? _showCallback;

    public InfoBarManager(Action<string, string, string>? showCallback = null)
    {
        _showCallback = showCallback;
    }

    public void Show(string title, string message, string severity = "Informational", int autoCloseSeconds = 5)
    {
        _showCallback?.Invoke(title, message, severity);
    }
}
