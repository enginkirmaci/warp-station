using System.Diagnostics;
using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services.OpenCode;

/// <inheritdoc cref="IOpenCodeModelService"/>
public class OpenCodeModelService : IOpenCodeModelService
{
    /// <summary>Upper bound for the <c>opencode models</c> call; a hung CLI must not stall the UI.</summary>
    private static readonly TimeSpan CliTimeout = TimeSpan.FromSeconds(15);

    /// <summary>Cache file holding the model list from the last successful CLI call.</summary>
    private static readonly string CacheFilePath = UserPaths.GetUserDataFile("opencode", "models.cache.json");

    /// <inheritdoc/>
    public IReadOnlyList<string> GetCachedModels()
    {
        try
        {
            if (!File.Exists(CacheFilePath))
                return Array.Empty<string>();

            return JsonSerializer.Deserialize<List<string>>(File.ReadAllText(CacheFilePath))
                ?? (IReadOnlyList<string>)Array.Empty<string>();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCodeModelService: failed to read the model cache");
            return Array.Empty<string>();
        }
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<string>> GetModelsAsync(string? executable, CancellationToken cancellationToken = default)
    {
        var exe = string.IsNullOrWhiteSpace(executable) ? "opencode" : executable;

        // The GUI process often runs with a minimal PATH (no shell rc files), so a bare
        // name must also be looked up in the user-level install dirs; spawning needs
        // the resolved absolute path either way.
        var resolved = ExecutableDefaults.Locate(exe);
        if (resolved is null)
        {
            Log.Logger.Warning(
                "OpenCodeModelService: '{Exe}' was not found on PATH or in the common user install folders (~/.local/bin, ~/.opencode/bin, …)",
                exe);
            return Array.Empty<string>();
        }

        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = resolved,
                Arguments = "models",
                UseShellExecute = false,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };

            // An Electron-hosted Tools (launched from VS Code & forks) leaks this variable
            // into children; it would degrade an Electron-packaged opencode to plain Node.
            psi.EnvironmentVariables.Remove("ELECTRON_RUN_AS_NODE");

            using var process = new Process { StartInfo = psi };
            process.Start();

            var outputTask = process.StandardOutput.ReadToEndAsync(cancellationToken);
            var completed = await Task.WhenAny(outputTask, Task.Delay(CliTimeout, cancellationToken));
            if (completed != outputTask)
            {
                try { process.Kill(entireProcessTree: true); } catch { /* best effort */ }
                Log.Logger.Warning("OpenCodeModelService: '{Exe} models' timed out after {Timeout}s", exe, CliTimeout.TotalSeconds);
                return Array.Empty<string>();
            }

            // Model ids are printed one per line as provider/model-id; the '/' guard drops any
            // stray non-model lines (banners, warnings leaked to stdout).
            var models = outputTask.Result
                .Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
                .Where(line => line.Contains('/'))
                .Distinct(StringComparer.Ordinal)
                .ToList();

            // Persist only a non-empty result so a transient CLI failure never clobbers a good cache.
            if (models.Count > 0)
                SaveCache(models);

            return models;
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCodeModelService: failed to list models via '{Exe} models'", exe);
            return Array.Empty<string>();
        }
    }

    /// <summary>Best-effort write of the model cache; never throws.</summary>
    private static void SaveCache(IReadOnlyList<string> models)
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(CacheFilePath)!);
            File.WriteAllText(CacheFilePath, JsonSerializer.Serialize(models));
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCodeModelService: failed to write the model cache");
        }
    }
}
