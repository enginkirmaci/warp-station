using System.Runtime.InteropServices;

namespace Tools.SnapIt.Contracts;

public class ScreenManager : IScreenManager
{
    private const uint WM_DISPLAYCHANGE = 126;
    private const uint WM_SETTINGCHANGE = 26;
    private static volatile bool screenChanged;

    private ISnapManager? snapManager;

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

        IsInitialized = true;
    }

    public void Dispose()
    {
        IsInitialized = false;
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
