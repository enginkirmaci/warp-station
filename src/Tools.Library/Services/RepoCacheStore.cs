using System.Text.Json;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-backed implementation of <see cref="IRepoCacheStore"/>. Reads and writes the
/// repo cache JSON located next to the executable under <c>settings/</c>.
/// </summary>
public class RepoCacheStore : IRepoCacheStore
{
    private static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };
    private static readonly JsonSerializerOptions WriteOptions = new() { WriteIndented = true };

    private readonly string _cacheFilePath;

    public RepoCacheStore()
    {
        // Mirror the settings.json location: <baseDirectory>/settings/repos.cache.json
        _cacheFilePath = Path.Combine(AppContext.BaseDirectory, "settings", "repos.cache.json");
    }

    /// <inheritdoc/>
    public async Task<RepoCache?> LoadAsync()
    {
        try
        {
            if (!File.Exists(_cacheFilePath))
                return null;

            var json = await File.ReadAllTextAsync(_cacheFilePath);
            return JsonSerializer.Deserialize<RepoCache>(json, ReadOptions);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error loading repo cache");
            return null;
        }
    }

    /// <inheritdoc/>
    public async Task SaveAsync(RepoCache cache)
    {
        try
        {
            var json = JsonSerializer.Serialize(cache, WriteOptions);
            await File.WriteAllTextAsync(_cacheFilePath, json);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error saving repo cache");
        }
    }
}
