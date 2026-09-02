using Avalonia;
using Serilog;

namespace Tools;

internal class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        try
        {
            BuildAvaloniaApp()
                .StartWithClassicDesktopLifetime(args);
        }
        catch (Exception ex)
        {
            Log.Fatal(ex, "Application terminated unexpectedly");
        }
        finally
        {
            Log.CloseAndFlush();
        }
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            // Must follow UsePlatformDetect: it captures the registered backend
            // initializer as its fallback. Prefers native Wayland (no XWayland) when the
            // session provides it; on TryInitialize failure it logs a warning and boots
            // the captured backend (X11 on Linux). No-op on non-Linux platforms.
            .UseWaylandWithFallback()
            .WithInterFont()
            .LogToTrace();
}