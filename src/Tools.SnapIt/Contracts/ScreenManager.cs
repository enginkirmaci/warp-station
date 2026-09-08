using System.Runtime.InteropServices;

namespace Tools.SnapIt.Contracts;

public class ScreenManager : IScreenManager
{
    private const uint WM_DISPLAYCHANGE = 126;
    private const uint WM_SETTINGCHANGE = 26;

    public bool IsInitialized { get; private set; }

    public void SetSnapManager(ISnapManager snapManager)
    {
        // Kept to satisfy the IScreenManager contract (SnapManager registers itself
        // during initialization), but nothing currently consumes the reference here.
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
}
