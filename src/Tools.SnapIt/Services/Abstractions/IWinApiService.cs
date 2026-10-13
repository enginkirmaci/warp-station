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

    /// <summary>
    /// Returns the outer window rectangle of the handle, or <see cref="Rectangle.Empty"/>
    /// when invalid. Unlike <see cref="GetWindowMargin"/> this takes only a handle, so it
    /// suits windows discovered after launch by <see cref="GetOpenWindows"/>.
    /// </summary>
    Rectangle GetWindowRect(nint handle);

    ActiveWindow GetActiveWindow();

    string GetCurrentDesktopWallpaper();

    void SetWindowCornerPreference(ActiveWindow activeWindow, DWM_WINDOW_CORNER_PREFERENCE preference);
}
