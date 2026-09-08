using Avalonia.Controls;
using Avalonia.Input;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

public class ClipboardService : IClipboardService
{
    private readonly IMainWindowProvider _mainWindowProvider;

    public ClipboardService(IMainWindowProvider mainWindowProvider)
    {
        _mainWindowProvider = mainWindowProvider;
    }

    public async void CopyText(string text)
    {
        // Fire-and-forget callers must not take the process down when the clipboard
        // backend fails (it can throw when the display/session is unavailable).
        try
        {
            await CopyTextAsync(text);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Copying text to the clipboard failed");
        }
    }

    public async Task CopyTextAsync(string text)
    {
        var clipboard = _mainWindowProvider.TopLevel?.Clipboard;
        if (clipboard is null)
        {
            Log.Logger.Warning("Clipboard: no window clipboard available to copy text to");
            return;
        }

        // Avalonia 12: plain text goes through the data-transfer API (SetTextAsync is gone)
        var transfer = new DataTransfer();
        transfer.Add(DataTransferItem.CreateText(text));
        await clipboard.SetDataAsync(transfer);
    }
}
