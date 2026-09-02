using Serilog;
using Tools.Library.Configuration;

namespace Tools.Helpers;

/// <summary>
/// Makes the native Wayland backend honor the session cursor theme.
/// <para>
/// Avalonia.Wayland loads cursors via <c>wl_cursor_theme_load(null, 24, …)</c>.
/// libwayland-cursor never reads <c>XCURSOR_THEME</c> — a null theme name resolves
/// to the hardcoded theme <c>"default"</c>, which on most distros inherits Adwaita,
/// so the app shows the wrong cursor shape. The package exposes no way to pass the
/// theme name (and ignores <c>XCURSOR_SIZE</c>), so <see cref="Apply"/> aliases
/// <c>"default"</c> to the session theme through a symlink inside an app-private
/// directory and puts that directory first on <c>XCURSOR_PATH</c>, which
/// libwayland-cursor reads at theme-load time. Must run before Avalonia
/// initializes its platform (i.e. before the first window is created).
/// </para>
/// </summary>
public static class WaylandCursorTheme
{
    /// <summary>
    /// Installs the cursor-theme alias for the current session. No-op on non-Linux
    /// platforms and outside Wayland sessions; never throws.
    /// </summary>
    public static void Apply()
    {
        if (!OperatingSystem.IsLinux())
            return;
        if (Environment.GetEnvironmentVariable("WAYLAND_DISPLAY") is null &&
            Environment.GetEnvironmentVariable("WAYLAND_SOCKET") is null)
            return;

        // XCURSOR_THEME is the session-wide setting; Hyprland setups sometimes only
        // export the hyprcursor variant, whose themes use the same directory names.
        var theme = Environment.GetEnvironmentVariable("XCURSOR_THEME");
        if (string.IsNullOrWhiteSpace(theme) || theme == "default")
            theme = Environment.GetEnvironmentVariable("HYPRCURSOR_THEME");
        if (string.IsNullOrWhiteSpace(theme) || theme == "default")
            return;

        var themeDir = LocateThemeDir(theme);
        if (themeDir is null)
        {
            Log.Debug("Wayland cursor theme {Theme}: no xcursor directory found, keeping default resolution", theme);
            return;
        }

        try
        {
            var shimRoot = Path.Combine(UserPaths.UserDataRoot, "cursor-shim");
            var shimLink = Path.Combine(shimRoot, "default");
            Directory.CreateDirectory(shimRoot);
            ReplaceSymbolicLink(shimLink, themeDir);

            var existing = Environment.GetEnvironmentVariable("XCURSOR_PATH");
            var searchPath = string.IsNullOrWhiteSpace(existing) ? DefaultSearchPath() : existing;
            Environment.SetEnvironmentVariable("XCURSOR_PATH", shimRoot + Path.PathSeparator + searchPath);
            Log.Debug("Wayland cursor theme {Theme} aliased via {ShimLink}", theme, shimLink);
        }
        catch (Exception ex)
        {
            // Cosmetic only: without the alias the app falls back to the "default" theme.
            Log.Debug(ex, "Wayland cursor theme alias failed for {Theme}", theme);
        }
    }

    /// <summary>
    /// Finds the xcursor directory of <paramref name="theme"/>, following
    /// <c>Inherits=</c> chains the same way xcursor theme resolution does.
    /// </summary>
    private static string? LocateThemeDir(string theme)
    {
        var roots = ThemeRoots();

        var visited = new HashSet<string>(StringComparer.Ordinal);
        var pending = new Queue<string>();
        pending.Enqueue(theme);
        while (pending.Count > 0)
        {
            var name = pending.Dequeue();
            if (!visited.Add(name))
                continue;
            foreach (var root in roots)
            {
                var dir = Path.Combine(root, name);
                if (Directory.Exists(Path.Combine(dir, "cursors")))
                    return dir;
                var inherits = ReadInheritedTheme(Path.Combine(dir, "index.theme"));
                if (inherits is not null)
                    pending.Enqueue(inherits);
            }
        }
        return null;
    }

    /// <summary>
    /// Absolute-path superset of libwayland-cursor's compiled-in search path
    /// (whose first entry is <c>$XDG_DATA_HOME/icons</c> or
    /// <c>~/.local/share/icons</c>). Only used when <c>XCURSOR_PATH</c> is not
    /// already set; the loader skips entries that don't exist.
    /// </summary>
    private static string DefaultSearchPath()
        => string.Join(Path.PathSeparator, ThemeRoots()
            .Append(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".cursors"))
            .Append("/usr/share/cursors/xorg-x11"));

    private static string[] ThemeRoots()
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var dataHome = Environment.GetEnvironmentVariable("XDG_DATA_HOME");
        if (string.IsNullOrWhiteSpace(dataHome))
            dataHome = Path.Combine(home, ".local", "share");
        return
        [
            Path.Combine(dataHome, "icons"),
            Path.Combine(home, ".icons"),
            "/usr/local/share/icons",
            "/usr/share/icons",
            "/usr/share/pixmaps",
        ];
    }

    /// <summary>First <c>Inherits=</c> entry of an index.theme, if any.</summary>
    private static string? ReadInheritedTheme(string indexTheme)
    {
        try
        {
            foreach (var line in File.ReadLines(indexTheme))
            {
                if (!line.StartsWith("Inherits=", StringComparison.OrdinalIgnoreCase))
                    continue;
                var first = line.Substring("Inherits=".Length).Split(',')[0].Trim();
                return first.Length > 0 ? first : null;
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
        }
        return null;
    }

    /// <summary>Points <paramref name="link"/> at <paramref name="target"/>, no-op when already correct.</summary>
    private static void ReplaceSymbolicLink(string link, string target)
    {
        var info = new FileInfo(link);
        if (string.Equals(info.LinkTarget, target, StringComparison.Ordinal))
            return;
        if (info.LinkTarget is not null || info.Exists)
            info.Delete(); // deletes the link itself, never the theme directory it points at
        else if (Directory.Exists(link))
            Directory.Delete(link, true); // not expected under app-private data
        File.CreateSymbolicLink(link, target);
    }
}
