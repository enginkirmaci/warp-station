using Avalonia;
using Avalonia.Controls;
using Avalonia.Input.Platform;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of clipboard service using Avalonia clipboard API.
/// </summary>
public class ClipboardService : IClipboardService
{
    /// <inheritdoc/>
    public async void CopyText(string text)
    {
        // Resolve clipboard from the active top-level window
        if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime lifetime)
        {
            var clipboard = lifetime.MainWindow?.Clipboard;
            if (clipboard != null)
            {
                await clipboard.SetTextAsync(text);
            }
        }
    }
}
