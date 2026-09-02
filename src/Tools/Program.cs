using Avalonia;
using Serilog;
using Tools.Helpers;

namespace Tools;

internal class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        // Must precede Avalonia startup: the Wayland platform loads its cursor
        // theme during initialization and ignores XCURSOR_THEME (the alias is
        // installed on disk — env vars don't reach native getenv).
        WaylandCursorTheme.Apply();

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