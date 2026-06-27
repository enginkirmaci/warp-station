using System.Text.Json;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-backed implementation of <see cref="IWorkspaceCacheStore"/>. Reads and writes
/// the workspace cache JSON located next to the executable under <c>settings/</c>.
/// </summary>
public class WorkspaceCacheStore : IWorkspaceCacheStore
{
    private static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };
    private static readonly JsonSerializerOptions WriteOptions = new() { WriteIndented = true };

    private readonly string _cacheFilePath;

    public WorkspaceCacheStore()
    {
        // Mirror the settings.json location: <baseDirectory>/settings/workspaces.cache.json
        _cacheFilePath = Path.Combine(AppContext.BaseDirectory, "settings", "workspaces.cache.json");
    }

    /// <inheritdoc/>
    public async Task<WorkspaceCache?> LoadAsync()
    {
        try
        {
            if (!File.Exists(_cacheFilePath))
                return null;

            var json = await File.ReadAllTextAsync(_cacheFilePath);
            return JsonSerializer.Deserialize<WorkspaceCache>(json, ReadOptions);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error loading workspace cache");
            return null;
        }
    }

    /// <inheritdoc/>
    public async Task SaveAsync(WorkspaceCache cache)
    {
        try
        {
            var json = JsonSerializer.Serialize(cache, WriteOptions);
            await File.WriteAllTextAsync(_cacheFilePath, json);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error saving workspace cache");
        }
    }
}
