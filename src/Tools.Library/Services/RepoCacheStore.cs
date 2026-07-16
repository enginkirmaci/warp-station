using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-backed implementation of <see cref="IRepoCacheStore"/>. Reads and writes the
/// repo cache JSON located in the user data folder under <c>%USERPROFILE%\.devtools\</c>.
/// </summary>
public class RepoCacheStore : IRepoCacheStore
{
    private static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };
    private static readonly JsonSerializerOptions WriteOptions = new() { WriteIndented = true };

    private readonly string _cacheFilePath;

    public RepoCacheStore()
    {
        // User data under %USERPROFILE%\.devtools so it survives reinstalls. The cache
        // is not seeded from shipped defaults — it rebuilds from the file system.
        _cacheFilePath = UserPaths.GetUserDataFile("repos.cache.json");
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
