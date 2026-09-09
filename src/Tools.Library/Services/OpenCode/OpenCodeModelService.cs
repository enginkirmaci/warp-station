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

    /// <summary>
    /// How long the in-memory catalog is reused before the next call re-runs the CLI
    /// (<see cref="GetModelsAsync"/>) or re-reads the cache file
    /// (<see cref="GetCachedModels"/>). Repeated drawer opens and quick launches within
    /// the window stop paying a process spawn / synchronous file read each.
    /// </summary>
    private static readonly TimeSpan CatalogLifetime = TimeSpan.FromMinutes(10);

    /// <summary>Cache file holding the model list from the last successful CLI call.</summary>
    private static readonly string CacheFilePath = UserPaths.GetUserDataFile("opencode", "models.cache.json");

    /// <summary>Memoized raw <c>opencode models</c> catalog (before the default-model
    /// merge) with the time it was taken; null until a CLI run first succeeds.</summary>
    private IReadOnlyList<string>? _cliCatalog;
    private DateTimeOffset _cliCatalogAt;

    /// <summary>Memoized cache-file content with the time it was read. A successful CLI run
    /// refreshes it directly (it just rewrote the file), so the memo mirrors the disk.</summary>
    private IReadOnlyList<string> _fileCatalog = Array.Empty<string>();
    private DateTimeOffset _fileCatalogAt;

    /// <inheritdoc/>
    public IReadOnlyList<string> GetCachedModels(string? defaultModel)
    {
        return MergeDefaultModel(ReadCacheCatalog(), defaultModel);
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<string>> GetModelsAsync(string? executable, string? defaultModel, bool forceRefresh = false, CancellationToken cancellationToken = default)
    {
        // TTL memoization: within the window the previous catalog answers without a
        // process spawn (re-merged with the current default); only an expired catalog —
        // or an explicit force refresh — runs the CLI again.
        if (!forceRefresh
            && _cliCatalog is not null
            && DateTimeOffset.UtcNow - _cliCatalogAt < CatalogLifetime)
        {
            return MergeDefaultModel(_cliCatalog, defaultModel);
        }

        // The GUI process often runs with a minimal PATH; LocateCli (see ProcessRunner)
        // resolves bare names against PATH plus the user-level install dirs.
        var (exe, resolved) = ProcessRunner.LocateCli(executable, ReposSettings.DefaultOpenCodeExecutable, "OpenCodeModelService");
        if (resolved is null)
        {
            return MergeDefaultModel(Array.Empty<string>(), defaultModel);
        }

        try
        {
            var result = await ProcessRunner.RunAsync(new ProcessRunOptions
            {
                FileName = resolved,
                Arguments = "models",
                Timeout = CliTimeout,
                StripElectronVariable = true,
            }, cancellationToken);

            if (result.TimedOut)
            {
                Log.Logger.Warning("OpenCodeModelService: '{Exe} models' timed out after {Timeout}s", exe, CliTimeout.TotalSeconds);
                return MergeDefaultModel(Array.Empty<string>(), defaultModel);
            }

            // Model ids are printed one per line as provider/model-id; the '/' guard drops any
            // stray non-model lines (banners, warnings leaked to stdout).
            var models = result.StandardOutput
                .Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
                .Where(line => line.Contains('/'))
                .Distinct(StringComparer.Ordinal)
                .ToList();

            // Persist only a non-empty result so a transient CLI failure never clobbers a good
            // cache; the persisted list already carries the default at the top (see below).
            if (models.Count > 0)
            {
                var merged = MergeDefaultModel(models, defaultModel);
                SaveCache(merged);
                RememberCatalog(models, merged);
                return merged;
            }

            return MergeDefaultModel(models, defaultModel);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCodeModelService: failed to list models via '{Exe} models'", exe);
            return MergeDefaultModel(Array.Empty<string>(), defaultModel);
        }
    }

    /// <inheritdoc/>
    public string ResolveLaunchModel(IReadOnlyList<string> models, string? defaultModel)
    {
        var model = defaultModel?.Trim();
        if (string.IsNullOrEmpty(model))
            return models.FirstOrDefault() ?? string.Empty;

        var match = models.FirstOrDefault(m => string.Equals(m, model, StringComparison.OrdinalIgnoreCase));
        return match ?? model;
    }

    /// <summary>
    /// Reads the model cache file, memoized for <see cref="CatalogLifetime"/> so repeated
    /// calls don't pay the synchronous read + deserialize each. External writes to the
    /// file become visible when the memo expires; a CLI run within this process refreshes
    /// the memo directly (see <see cref="RememberCatalog"/>). Never throws.
    /// </summary>
    private IReadOnlyList<string> ReadCacheCatalog()
    {
        if (DateTimeOffset.UtcNow - _fileCatalogAt < CatalogLifetime)
        {
            return _fileCatalog;
        }

        try
        {
            if (!File.Exists(CacheFilePath))
            {
                _fileCatalog = Array.Empty<string>();
            }
            else
            {
                _fileCatalog = JsonSerializer.Deserialize<List<string>>(File.ReadAllText(CacheFilePath))
                    ?? (IReadOnlyList<string>)Array.Empty<string>();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCodeModelService: failed to read the model cache");
            _fileCatalog = Array.Empty<string>();
        }

        _fileCatalogAt = DateTimeOffset.UtcNow;
        return _fileCatalog;
    }

    /// <summary>
    /// Refreshes both memos after a successful CLI run: the raw catalog answers subsequent
    /// <see cref="GetModelsAsync"/> calls within the TTL, and the merged list — exactly
    /// what <see cref="SaveCache"/> just wrote — answers <see cref="GetCachedModels"/>
    /// without re-reading the file.
    /// </summary>
    private void RememberCatalog(IReadOnlyList<string> cliCatalog, IReadOnlyList<string> mergedCatalog)
    {
        var now = DateTimeOffset.UtcNow;
        _cliCatalog = cliCatalog;
        _cliCatalogAt = now;
        _fileCatalog = mergedCatalog;
        _fileCatalogAt = now;
    }

    /// <summary>
    /// Ensures the configured default model is the FIRST entry of <paramref name="models"/>:
    /// prepended when missing, rotated to the front when already listed further down (a cache
    /// written before the default was picked would otherwise leave it buried and every
    /// FirstOrDefault fallback would resolve to the catalog's own first entry). Matched
    /// case-insensitively against the CLI's own casing. Returns <paramref name="models"/>
    /// unchanged when no default is configured or it already leads the list.
    /// </summary>
    private static IReadOnlyList<string> MergeDefaultModel(IReadOnlyList<string> models, string? defaultModel)
    {
        var model = defaultModel?.Trim();
        if (string.IsNullOrEmpty(model))
            return models;

        var index = -1;
        for (var i = 0; i < models.Count; i++)
        {
            if (string.Equals(models[i], model, StringComparison.OrdinalIgnoreCase))
            {
                index = i;
                break;
            }
        }

        if (index < 0)
        {
            var merged = new List<string>(models.Count + 1) { model };
            merged.AddRange(models);
            return merged;
        }

        if (index == 0)
            return models;

        var reordered = new List<string>(models.Count) { models[index] };
        for (var i = 0; i < models.Count; i++)
        {
            if (i != index)
                reordered.Add(models[i]);
        }
        return reordered;
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
