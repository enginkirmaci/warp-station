#if WINDOWS
using Avalonia.Controls;

namespace Tools.Helpers;

public sealed class WindowConfigurator
{
    private readonly Window _window;
    private nint _hwnd;
    
    public nint WindowHandle => _hwnd;

    public WindowConfigurator(Window window)
    {
        _window = window;
    }

    public void Configure()
    {
        var handle = _window.TryGetPlatformHandle();
        if (handle != null) _hwnd = handle.Handle;
    }
}
#else
using Avalonia.Controls;

namespace Tools.Helpers;

public sealed class WindowConfigurator
{
    public nint WindowHandle => nint.Zero;
    public WindowConfigurator(Window window) { }
    public void Configure() { }
}
#endif
