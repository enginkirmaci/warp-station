using Tools.Library.Services.Abstractions;
using Windows.ApplicationModel.DataTransfer;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of clipboard service for copying text.
/// </summary>
public class ClipboardService : IClipboardService
{
    /// <inheritdoc/>
    public void CopyText(string text)
    {
        var dataPackage = new DataPackage();
        dataPackage.SetText(text);
        Clipboard.SetContent(dataPackage);
    }
}
