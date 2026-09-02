using System.Collections.Concurrent;

namespace Tools.Library.Services;

/// <summary>
/// Resolves the effective executables for repo actions (terminal, .NET IDE) from the
/// configured settings, applying per-platform defaults. On Linux the persisted Windows
/// defaults cannot run, so well-known native terminals/IDEs are probed on PATH; the
/// first match wins. Returns null when nothing usable could be resolved, so callers
/// can surface a "configure it in settings" hint instead of a failed spawn.
/// Also provides <see cref="Locate"/> for spawning CLIs (e.g. opencode) that a GUI
/// process cannot otherwise find: Linux GUI sessions inherit a minimal PATH that does
/// not include the user-level bin directories shell rc files add.
/// <para>
/// Probe results are memoized per process: executables don't move while the app runs,
/// and the lookups behind <see cref="Locate"/> (PATH/user-bin walks, desktop-entry and
/// AppImage folder scans) and <see cref="HasVisualStudio"/> (a vswhere process) are not
/// free — availability is checked once per name, not on every page visit or launch.
/// A changed configured value probes afresh; uninstalled-mid-session tools are picked
/// up on the next app start.
/// </para>
/// </summary>
public static class ExecutableDefaults
{
    // Preference-ordered well-known terminal emulators. Each has argument support in
    // TerminalArgumentFormatter; an unknown fallback cannot receive a working directory.
    private static readonly string[] KnownLinuxTerminals =
    [
        "konsole",
        "gnome-terminal",
        "xfce4-terminal",
        "alacritty",
        "kitty",
        "wezterm",
        "xterm",
        "uxterm"
    ];

    // Preference-ordered well-known .NET IDEs (the Linux stand-ins for the Visual
    // Studio "open solution" action).
    private static readonly string[] KnownLinuxIdes =
    [
        "rider",
        "jetbrains-rider",
        "codium",
        "code"
    ];

    // User-level install locations that GUI sessions do not have on PATH, because the
    // display manager sets PATH without sourcing shell rc files. Searched in addition
    // to PATH when resolving a bare executable name.
    private static readonly string[] UserBinDirectories =
    [
        ".local/bin",
        "bin",
        ".cargo/bin",
        ".bun/bin",
        ".opencode/bin",
        ".npm-global/bin",
        ".local/share/pnpm",
        ".yarn/bin",
        ".config/yarn/global/node_modules/.bin",
    ];

    // --- Per-process probe memos (see the class doc) ---

    /// <summary>Bare-name <see cref="Locate"/> resolutions, negative results included;
    /// AppImage/desktop-entry probes land here too.</summary>
    private static readonly ConcurrentDictionary<string, string?> LocatedByName = new(StringComparer.Ordinal);

    /// <summary>Memoized <see cref="HasVisualStudio"/> probe (null = not probed yet).</summary>
    private static bool? _hasVisualStudio;

    /// <summary>Memoized Linux terminal auto-detect; the Done flag distinguishes "not probed"
    /// from "probed and found nothing".</summary>
    private static string? _detectedTerminal;
    private static bool _terminalDetectionDone;

    /// <summary>Memoized Linux .NET IDE auto-detect (same pattern as the terminal).</summary>
    private static string? _detectedIde;
    private static bool _ideDetectionDone;

    /// <summary>
    /// Resolves the terminal emulator to use. On Windows this is the configured value
    /// (defaulting to Windows Terminal); on Linux the persisted Windows default
    /// ("wt"/"wt.exe") and an empty value both trigger PATH auto-detection.
    /// </summary>
    /// <param name="configured">The configured terminal executable, if any.</param>
    /// <returns>The executable (absolute path where detected), or null when none could be resolved.</returns>
    public static string? ResolveTerminal(string? configured)
    {
        var trimmed = configured?.Trim();

        if (OperatingSystem.IsWindows())
        {
            return string.IsNullOrWhiteSpace(trimmed) ? "wt" : trimmed;
        }

        // "wt" is the shipped settings default and cannot run here — treat it as unset.
        if (!string.IsNullOrWhiteSpace(trimmed)
            && !trimmed.Equals("wt", StringComparison.OrdinalIgnoreCase)
            && !trimmed.Equals("wt.exe", StringComparison.OrdinalIgnoreCase))
        {
            return trimmed;
        }

        var detected = DetectTerminalOnce();
        if (detected is null)
        {
            Serilog.Log.Logger.Warning(
                "No known terminal emulator found on PATH (tried: {Candidates}); set the terminal executable in Repos settings",
                string.Join(", ", KnownLinuxTerminals));
        }

        return detected;
    }

    /// <summary>
    /// Resolves the IDE used to open solutions. On Windows the .sln shell association is
    /// kept unless an IDE is explicitly configured (null means "use the association");
    /// on other platforms a well-known .NET IDE is auto-detected from PATH.
    /// </summary>
    /// <param name="configured">The configured IDE executable, if any.</param>
    /// <returns>
    /// The executable (absolute path where detected), or null when none is
    /// configured/detected (on Windows the caller then falls back to the .sln shell
    /// association).
    /// </returns>
    public static string? ResolveIde(string? configured)
    {
        var trimmed = configured?.Trim();
        if (!string.IsNullOrWhiteSpace(trimmed))
        {
            return trimmed;
        }

        if (OperatingSystem.IsWindows())
        {
            return null;
        }

        var detected = DetectIdeOnce();
        if (detected is null)
        {
            Serilog.Log.Logger.Warning(
                "No known .NET IDE found on PATH (tried: {Candidates}); set the IDE executable in Repos settings",
                string.Join(", ", KnownLinuxIdes));
        }

        return detected;
    }

    /// <summary>
    /// Locates an executable for direct spawning. Paths (containing a separator) are
    /// returned verbatim; bare names on Windows are left for CreateProcess to resolve
    /// against PATH/PATHEXT; bare names on other platforms are resolved to an absolute
    /// path by searching PATH plus the user-level bin directories rc files usually add,
    /// and finally AppImage installs (desktop-entry integration, then common folders) —
    /// an AppImage never appears on PATH as a bare executable.
    /// </summary>
    /// <param name="executable">The configured executable name or path.</param>
    /// <returns>The executable to spawn, or null when a bare name could not be found.</returns>
    public static string? Locate(string? executable)
    {
        var trimmed = executable?.Trim();
        if (string.IsNullOrWhiteSpace(trimmed))
        {
            return null;
        }

        if (trimmed.Contains('/') || trimmed.Contains('\\'))
        {
            return trimmed;
        }

        if (OperatingSystem.IsWindows())
        {
            return trimmed;
        }

        return LocatedByName.GetOrAdd(trimmed, static name => FindExecutableFile(name) ?? FindAppImage(name));
    }

    /// <summary>First-run probe of the well-known terminals, memoized for the process.</summary>
    private static string? DetectTerminalOnce()
    {
        if (!_terminalDetectionDone)
        {
            _detectedTerminal = DetectOnPath(KnownLinuxTerminals);
            _terminalDetectionDone = true;
        }

        return _detectedTerminal;
    }

    /// <summary>First-run probe of the well-known .NET IDEs, memoized for the process.</summary>
    private static string? DetectIdeOnce()
    {
        if (!_ideDetectionDone)
        {
            _detectedIde = DetectOnPath(KnownLinuxIdes);
            _ideDetectionDone = true;
        }

        return _detectedIde;
    }

    /// <summary>
    /// Whether Visual Studio — a product edition that can open solutions — is installed.
    /// Probes the <c>vswhere.exe</c> that ships with the Visual Studio installer for the
    /// latest instance (default products: Community/Professional/Enterprise, previews
    /// included; Build Tools excluded). On non-Windows platforms Visual Studio does not
    /// exist, so this returns <see langword="false"/> without probing.
    /// </summary>
    public static bool HasVisualStudio()
    {
        if (!OperatingSystem.IsWindows())
        {
            return false;
        }

        if (_hasVisualStudio is { } cached)
        {
            return cached;
        }

        _hasVisualStudio = ProbeVisualStudio();
        return _hasVisualStudio.Value;
    }

    /// <summary>Runs the vswhere probe once; callers memoize the outcome.</summary>
    private static bool ProbeVisualStudio()
    {
        var vswhere = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.ProgramFilesX86),
            "Microsoft Visual Studio",
            "Installer",
            "vswhere.exe");
        if (!File.Exists(vswhere))
        {
            return false;
        }

        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = vswhere,
                Arguments = "-latest -prerelease -property installationPath -format value",
                UseShellExecute = false,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };

            using var process = Process.Start(psi)!;
            var output = process.StandardOutput.ReadToEnd();
            process.WaitForExit(3000);

            // A reported installation path is the presence proof; verify it still exists
            // so a stale installer cache (uninstalled VS) does not count.
            var installPath = output.Trim();
            return !string.IsNullOrWhiteSpace(installPath) && Directory.Exists(installPath);
        }
        catch (Exception ex)
        {
            Serilog.Log.Logger.Warning(ex, "ExecutableDefaults: vswhere probe failed; treating Visual Studio as not installed");
            return false;
        }
    }

    private static string? DetectOnPath(string[] candidates)
    {
        foreach (var candidate in candidates)
        {
            var found = FindExecutableFile(candidate);
            if (found is not null)
            {
                return found;
            }
        }

        return null;
    }

    private static string? FindExecutableFile(string fileName)
    {
        foreach (var directory in EnumerateSearchDirectories())
        {
            var candidate = Path.Combine(directory, fileName);
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }

        return null;
    }

    // Directories (relative to the profile folder) commonly used to keep AppImages.
    private static readonly string[] AppImageDirectories =
    [
        "Applications",
        "AppImages",
        "Apps",
        "Downloads",
        ".local/bin",
    ];

    // Desktop entry folders searched for an AppImage's integrated launcher, in addition
    // to the system-wide /usr/share/applications.
    private static readonly string[] DesktopEntryDirectories =
    [
        ".local/share/applications",
    ];

    /// <summary>
    /// Resolves an AppImage install of <paramref name="name"/>. Desktop-entry
    /// integrations point at the actual file, so those are read first; otherwise the
    /// common AppImage folders are probed for "*&lt;name&gt;*.appimage", preferring the
    /// most recently updated file when several versions sit side by side.
    /// </summary>
    private static string? FindAppImage(string name)
    {
        return FindDesktopEntryTarget(name) ?? ProbeAppImageDirectories(name);
    }

    private static string? FindDesktopEntryTarget(string name)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (string.IsNullOrEmpty(home))
        {
            return null;
        }

        foreach (var directory in DesktopEntryDirectories
                     .Select(relative => Path.Combine(home, relative))
                     .Append("/usr/share/applications"))
        {
            if (!Directory.Exists(directory))
            {
                continue;
            }

            string[] files;
            try
            {
                files = Directory.GetFiles(directory, "*.desktop");
            }
            catch (Exception)
            {
                continue;
            }

            foreach (var file in files.Where(file => Path.GetFileName(file).Contains(name, StringComparison.OrdinalIgnoreCase)))
            {
                var target = ReadDesktopExecTarget(file);
                if (target is not null && File.Exists(target))
                {
                    return target;
                }
            }
        }

        return null;
    }

    /// <summary>Reads the executable targeted by a desktop entry's Exec line, if any.</summary>
    private static string? ReadDesktopExecTarget(string desktopFile)
    {
        try
        {
            foreach (var line in File.ReadLines(desktopFile))
            {
                if (!line.StartsWith("Exec=", StringComparison.Ordinal))
                {
                    continue;
                }

                var token = FirstExecToken(line["Exec=".Length..].Trim());
                // A bare field code (e.g. "Exec=%f") names no executable.
                return string.IsNullOrWhiteSpace(token) || token.Contains('%') ? null : token;
            }
        }
        catch (Exception)
        {
            // Best effort: an unreadable desktop entry simply contributes no target.
        }

        return null;
    }

    /// <summary>Reads the first Exec token, honoring quotes per the desktop entry spec.</summary>
    private static string? FirstExecToken(string exec)
    {
        exec = exec.TrimStart();
        if (exec.Length == 0)
        {
            return null;
        }

        if (exec[0] is not ('"' or '\''))
        {
            return exec.Split(' ', 2)[0];
        }

        var end = exec.IndexOf(exec[0], 1);
        return end < 0 ? null : exec[1..end].Replace("\\\\", "\\").Replace($"\\{exec[0]}", exec[0].ToString());
    }

    private static string? ProbeAppImageDirectories(string name)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (string.IsNullOrEmpty(home))
        {
            return null;
        }

        foreach (var relative in AppImageDirectories)
        {
            var directory = Path.Combine(home, relative);
            if (!Directory.Exists(directory))
            {
                continue;
            }

            string? newest = null;
            var newestWrite = DateTime.MinValue;
            try
            {
                foreach (var file in Directory.EnumerateFiles(
                             directory,
                             "*.appimage",
                             new EnumerationOptions { IgnoreInaccessible = true, MatchCasing = MatchCasing.CaseInsensitive }))
                {
                    if (!Path.GetFileName(file).Contains(name, StringComparison.OrdinalIgnoreCase))
                    {
                        continue;
                    }

                    var write = File.GetLastWriteTimeUtc(file);
                    if (write <= newestWrite)
                    {
                        continue;
                    }

                    newestWrite = write;
                    newest = file;
                }
            }
            catch (Exception)
            {
                // Best effort: an unreadable folder simply contributes no candidate.
            }

            if (newest is not null)
            {
                return newest;
            }
        }

        return null;
    }

    private static IEnumerable<string> EnumerateSearchDirectories()
    {
        var seen = new HashSet<string>(StringComparer.Ordinal);

        var pathVariable = Environment.GetEnvironmentVariable("PATH");
        if (!string.IsNullOrWhiteSpace(pathVariable))
        {
            foreach (var directory in pathVariable.Split(
                         Path.PathSeparator,
                         StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
            {
                if (seen.Add(directory))
                {
                    yield return directory;
                }
            }
        }

        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        if (string.IsNullOrEmpty(home))
        {
            yield break;
        }

        foreach (var relative in UserBinDirectories)
        {
            var directory = Path.Combine(home, relative);
            if (seen.Add(directory))
            {
                yield return directory;
            }
        }

        // nvm keeps each installed Node version's executables in its own bin directory.
        var nvmVersions = Path.Combine(home, ".nvm", "versions", "node");
        if (Directory.Exists(nvmVersions))
        {
            foreach (var version in Directory.GetDirectories(nvmVersions))
            {
                var directory = Path.Combine(version, "bin");
                if (seen.Add(directory))
                {
                    yield return directory;
                }
            }
        }
    }
}
