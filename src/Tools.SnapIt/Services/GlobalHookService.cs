using SharpHook;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Services;

public class GlobalHookService : IGlobalHookService
{
    private volatile bool isDisposed;

    public SimpleGlobalHook? Hook { get; set; }

    public bool IsInitialized { get; private set; }

    public GlobalHookService()
    {
    }

    public async Task InitializeAsync()
    {
        if (IsInitialized)
        {
            return;
        }

        Hook = new SimpleGlobalHook();

        Task.Run(() =>
        {
            if (!isDisposed && Hook != null && !Hook.IsRunning)
            {
                Hook.Run();
            }
        });

        IsInitialized = true;
    }

    public void Dispose()
    {
        isDisposed = true;
        Hook?.Dispose();
    }
}
