using Avalonia;
using Avalonia.Controls;
using Avalonia.Input.Platform;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

public class ClipboardService : IClipboardService
{
    public async void CopyText(string text)
    {
        if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime lifetime
            && lifetime.MainWindow != null)
        {
            var clipboard = TopLevel.GetTopLevel(lifetime.MainWindow)?.Clipboard;
            if (clipboard != null)
            {
                await clipboard.SetTextAsync(text);
            }
        }
    }
}
