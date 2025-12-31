using Windows.ApplicationModel.DataTransfer;

namespace Tools.Services;

public interface IClipboardService
{
    void CopyText(string text);
}

public class ClipboardService : IClipboardService
{
    public void CopyText(string text)
    {
        var dataPackage = new DataPackage();
        dataPackage.SetText(text);
        Clipboard.SetContent(dataPackage);
    }
}
