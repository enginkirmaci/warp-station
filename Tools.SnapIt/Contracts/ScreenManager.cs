using System.Windows.Interop;

namespace Tools.SnapIt.Contracts;

public class ScreenManager : IScreenManager
{
    private const uint WM_DISPLAYCHANGE = 126;
    private const uint WM_SETTINGCHANGE = 26;
    private static volatile bool screenChanged;

    private ISnapManager? snapManager;
    private HwndSource? hwndSource;

    public bool IsInitialized { get; private set; }

    public void SetSnapManager(ISnapManager snapManager)
    {
        this.snapManager = snapManager;
    }

    public async Task InitializeAsync()
    {
        if (IsInitialized)
        {
            return;
        }

        hwndSource = HwndSource.FromHwnd(new WindowInteropHelper(System.Windows.Application.Current.MainWindow).Handle);
        hwndSource.AddHook(new HwndSourceHook(WndProc));

        IsInitialized = true;
    }

    public void Dispose()
    {
        if (hwndSource != null)
        {
            hwndSource.RemoveHook(new HwndSourceHook(WndProc));
            hwndSource = null;
        }

        IsInitialized = false;
    }

    private nint WndProc(nint hwnd, int msg, nint wParam, nint lParam, ref bool handled)
    {
        switch ((uint)msg)
        {
            case WM_DISPLAYCHANGE:
                Dev.Log("WM_DISPLAYCHANGE");
                screenChanged = true;
                ScreenChangedTask();

                break;

            case WM_SETTINGCHANGE:
                screenChanged = true;
                ScreenChangedTask();

                Dev.Log("WM_SETTINGCHANGE");

                break;
        }

        return nint.Zero;
    }

    private async void ScreenChangedTask()
    {
        if (screenChanged)
        {
            screenChanged = false;
            snapManager?.ScreenChangedEvent();
        }
    }
}