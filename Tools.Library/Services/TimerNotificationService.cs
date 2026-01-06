using System.Runtime.InteropServices;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerNotificationService.
/// </summary>
public class TimerNotificationService : ITimerNotificationService
{
    public event EventHandler<bool>? VisibilityRequested;

    public void PlayNotificationSound()
    {
        try
        {
            MessageBeep(0x00000040); // MB_ICONINFORMATION
        }
        catch
        {
            // Ignore sound errors
        }
    }

    public void RequestWindowVisibility(bool show)
    {
        VisibilityRequested?.Invoke(this, show);
    }

    [DllImport("user32.dll")]
    private static extern bool MessageBeep(uint uType);
}
