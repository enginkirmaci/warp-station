using System.Collections.Concurrent;
using System.Text.RegularExpressions;
using Avalonia.Platform;
using Serilog;

namespace Tools.Library.Services;

/// <summary>
/// Loads SVG path data from .svg asset files embedded in the application.
/// Assets are resolved via the Avalonia asset loader using the
/// <c>avares://{AssemblyName}/Assets/{name}.svg</c> URI scheme.
/// </summary>
public static class IconAssetLoader
{
    private const string DefaultAssemblyName = "Tools";
    private const string AssetsFolder = "Assets";
    private static readonly Regex PathAttributeRegex = new(
        """<path[^>]*\bd\s*=\s*["']([^"']+)["']""",
        RegexOptions.IgnoreCase | RegexOptions.Compiled);

    private static readonly ConcurrentDictionary<string, string> PathCache = new(StringComparer.OrdinalIgnoreCase);
    private static string _assemblyName = DefaultAssemblyName;
    private static bool _initialized;

    /// <summary>
    /// Known icon asset names (without the .svg extension). Keep in sync with files under Assets/.
    /// </summary>
    public static IReadOnlyCollection<string> KnownIcons { get; } = new[]
    {
        "icon-account",
        "icon-chart-bar",
        "icon-chevron-down",
        "icon-clipboard",
        "icon-close",
        "icon-clock",
        "icon-cog",
        "icon-database",
        "icon-folder",
        "icon-folder-alt",
        "icon-grid",
        "icon-home",
        "icon-lock",
        "icon-menu",
        "icon-package",
        "icon-play",
        "icon-refresh",
        "icon-sln-box",
        "icon-stop",
        "icon-terminal",
        "icon-terminal-alt",
        "icon-text-format",
        "icon-vscode",
    };

    /// <summary>
    /// Configures the assembly name used to resolve asset URIs. Call once at app startup
    /// if the consuming assembly is not named "Tools".
    /// </summary>
    public static void Configure(string assemblyName)
    {
        if (string.IsNullOrWhiteSpace(assemblyName))
        {
            throw new ArgumentException("Assembly name must be provided.", nameof(assemblyName));
        }

        _assemblyName = assemblyName;
        _initialized = true;
    }

    /// <summary>
    /// Returns the SVG path data string for the given icon (e.g. "icon-clipboard").
    /// Returns an empty string and logs a warning if the asset cannot be loaded.
    /// </summary>
    public static string GetPathData(string name)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);

        if (PathCache.TryGetValue(name, out var cached))
        {
            return cached;
        }

        var pathData = LoadPathDataFromAsset(name);
        PathCache[name] = pathData;
        return pathData;
    }

    /// <summary>
    /// Pre-loads every known icon. Optional; the loader is lazy by default.
    /// </summary>
    public static void PreloadAll()
    {
        foreach (var name in KnownIcons)
        {
            _ = GetPathData(name);
        }
    }

    /// <summary>
    /// Clears the in-memory cache so files can be re-read (useful for tests).
    /// </summary>
    public static void ClearCache()
    {
        PathCache.Clear();
    }

    private static string LoadPathDataFromAsset(string name)
    {
        try
        {
            var uri = new Uri($"avares://{_assemblyName}/{AssetsFolder}/{name}.svg");
            using var stream = AssetLoader.Open(uri);
            using var reader = new StreamReader(stream);
            var content = reader.ReadToEnd();
            var match = PathAttributeRegex.Match(content);
            if (match.Success)
            {
                return match.Groups[1].Value;
            }

            Log.Logger.Warning("No 'd' attribute found in '{Name}.svg'", name);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to load '{Name}.svg'", name);
        }

        return string.Empty;
    }
}
