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
/// theme name, and the alias cannot be carried in an environment variable:
/// <see cref="Environment.SetEnvironmentVariable"/> is not visible to native
/// <c>getenv</c> (verified — the managed value never reaches libwayland-cursor).
/// Instead <see cref="Apply"/> aliases <c>"default"</c> on disk, in the XDG icons
/// root (<c>$XDG_DATA_HOME/icons/default</c>, falling back to
/// <c>~/.local/share/icons/default</c>) — the first entry of the library's
/// compiled-in search path — pointing it at the session theme's directory. Must run
/// before Avalonia initializes its platform; no-op on non-Linux platforms and
/// outside Wayland sessions.
/// </para>
/// </summary>
public static class WaylandCursorTheme
{
    /// <summary>
    /// Installs the cursor-theme alias for the current session. No-op when the
    /// session theme is unset, already <c>"default"</c>, or unresolvable — and
    /// never overrides an existing on-disk <c>default</c> entry; never throws.
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
            TryCleanupOldShim();
            var aliasLink = Path.Combine(XdgIconsRoot(), "default");
            AliasDefaultTheme(aliasLink, themeDir);
        }
        catch (Exception ex)
        {
            // Cosmetic only: without the alias the app falls back to the "default" theme.
            Log.Debug(ex, "Wayland cursor theme alias failed for {Theme}", theme);
        }
    }

    /// <summary>
    /// Points <paramref name="aliasLink"/> at <paramref name="themeDir"/> unless it
    /// already resolves to a theme on its own: an existing real directory or a
    /// symlink aimed at a live directory is the user's own configuration and wins;
    /// only a missing or dangling entry gets (re)created.
    /// </summary>
    private static void AliasDefaultTheme(string aliasLink, string themeDir)
    {
        var info = new FileInfo(aliasLink);
        if (info.LinkTarget is { } target)
        {
            if (string.Equals(target, themeDir, StringComparison.Ordinal))
                return;
            if (Directory.Exists(target) || File.Exists(target))
            {
                Log.Debug("Wayland cursor theme: on-disk default already points at {Target}, leaving it", target);
                return;
            }
            info.Delete(); // dangling link — deletes the link itself, never a theme directory
        }
        else if (info.Exists || Directory.Exists(aliasLink))
        {
            Log.Debug("Wayland cursor theme: on-disk default at {AliasLink} already exists, leaving it", aliasLink);
            return;
        }
        File.CreateSymbolicLink(aliasLink, themeDir);
        Log.Debug("Wayland cursor theme {Theme} aliased as default via {AliasLink}", themeDir, aliasLink);
    }

    /// <summary>Removes the superseded XCURSOR_PATH-era shim directory, if present.</summary>
    private static void TryCleanupOldShim()
    {
        try
        {
            Directory.Delete(Path.Combine(UserPaths.UserDataRoot, "cursor-shim"), recursive: true);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
        }
    }

    /// <summary>
    /// Finds the xcursor directory of <paramref name="theme"/>, following
    /// <c>Inherits=</c> chains the same way xcursor theme resolution does.
    /// </summary>
    private static string? LocateThemeDir(string theme)
    {
        var visited = new HashSet<string>(StringComparer.Ordinal);
        var pending = new Queue<string>();
        pending.Enqueue(theme);
        while (pending.Count > 0)
        {
            var name = pending.Dequeue();
            if (!visited.Add(name))
                continue;
            foreach (var root in ThemeRoots())
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
    /// The directories libwayland-cursor searches, in its own precedence order:
    /// <c>$XDG_DATA_HOME/icons</c> (else <c>~/.local/share/icons</c>), then
    /// <c>~/.icons</c> and the system roots. The loader skips missing entries.
    /// </summary>
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

    private static string XdgIconsRoot() => ThemeRoots()[0];

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
}
