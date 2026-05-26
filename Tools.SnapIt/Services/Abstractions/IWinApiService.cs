using Tools.SnapIt.Contracts;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;

namespace Tools.SnapIt.Services.Abstractions;

public interface IWinApiService : IInitialize
{
    IDictionary<nint, string> GetOpenWindows();

    IEnumerable<string> GetOpenWindowsNames();

    bool IsFullscreen(ActiveWindow activeWindow);

    bool IsAllowedWindowStyle(ActiveWindow activeWindow);

    void MoveWindow(ActiveWindow activeWindow, Rectangle newRect);

    void MoveWindow(ActiveWindow activeWindow, int X, int Y, int width, int height);

    void SendMessage(ActiveWindow activeWindow);

    void GetWindowMargin(ActiveWindow activeWindow, out Rectangle withMargin);

    ActiveWindow GetActiveWindow();

    string GetCurrentDesktopWallpaper();

    void SetWindowCornerPreference(ActiveWindow activeWindow, DWM_WINDOW_CORNER_PREFERENCE preference);
}
