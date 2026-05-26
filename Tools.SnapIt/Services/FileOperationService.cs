using System.Collections.Concurrent;
using System.IO;
using Tools.SnapIt.Common.Entities;
using Tools.SnapIt.Common.Helpers;
using Tools.SnapIt.Services.Contracts;

namespace Tools.SnapIt.Services;

public class FileOperationService : IFileOperationService
{
    private const string LayoutFolder = "Layoutsv20";

    private readonly string _rootFolder = Constants.RootFolder;
    private readonly SemaphoreSlim _initLock = new(1, 1);
    private readonly ConcurrentDictionary<string, SemaphoreSlim> _fileLocks = new();
    private volatile bool _isInitialized;

    public bool IsInitialized => _isInitialized;

    public async Task InitializeAsync()
    {
        if (_isInitialized)
        {
            return;
        }

        await _initLock.WaitAsync();
        try
        {
            if (_isInitialized)
            {
                return;
            }

            Directory.CreateDirectory(_rootFolder);
            _isInitialized = true;
        }
        finally
        {
            _initLock.Release();
        }
    }

    public void Dispose()
    {
        _isInitialized = false;
        _initLock?.Dispose();

        foreach (var lockObj in _fileLocks.Values)
        {
            lockObj?.Dispose();
        }
        _fileLocks.Clear();
    }

    public async Task SaveAsync<T>(T config)
    {
        var configPath = GetConfigPath<T>();
        var fileLock = GetFileLock(configPath);

        await fileLock.WaitAsync();
        try
        {
            var json = Json.Serialize(config);
            await File.WriteAllTextAsync(configPath, json);
        }
        finally
        {
            fileLock.Release();
        }
    }

    public async Task<T> LoadAsync<T>() where T : new()
    {
        var configPath = GetConfigPath<T>();
        var fileLock = GetFileLock(configPath);

        await fileLock.WaitAsync();
        try
        {
            if (!File.Exists(configPath))
            {
                var defaultJson = Json.Serialize(new T());
                await File.WriteAllTextAsync(configPath, defaultJson);
                return new T();
            }

            var json = await File.ReadAllTextAsync(configPath);
            return Json.Deserialize<T>(json);
        }
        finally
        {
            fileLock.Release();
        }
    }

    public void SaveLayout(Layout layout)
    {
        var layoutPath = GetLayoutPath(layout);
        var fileLock = GetFileLock(layoutPath);

        fileLock.Wait();
        try
        {
            var json = Json.Serialize(layout);
            File.WriteAllText(layoutPath, json);
        }
        finally
        {
            fileLock.Release();
        }
    }

    public void ExportLayout(Layout layout, string layoutPath)
    {
        var fileLock = GetFileLock(layoutPath);

        fileLock.Wait();
        try
        {
            var json = Json.Serialize(layout);
            File.WriteAllText(layoutPath, json);
        }
        finally
        {
            fileLock.Release();
        }
    }

    public void DeleteLayout(Layout layout)
    {
        var layoutPath = GetLayoutPath(layout);
        var fileLock = GetFileLock(layoutPath);

        fileLock.Wait();
        try
        {
            if (File.Exists(layoutPath))
            {
                File.Delete(layoutPath);
            }
        }
        finally
        {
            fileLock.Release();
            _fileLocks.TryRemove(layoutPath, out _);
        }
    }

    public Layout ImportLayout(string layoutPath)
    {
        var fileLock = GetFileLock(layoutPath);

        fileLock.Wait();
        try
        {
            var json = File.ReadAllText(layoutPath);
            var layout = Json.Deserialize<Layout>(json);
            SaveLayout(layout);
            return layout;
        }
        finally
        {
            fileLock.Release();
        }
    }

    public IList<Layout> GetLayouts()
    {
        var folderPath = Path.Combine(_rootFolder, LayoutFolder);

        if (!Directory.Exists(folderPath))
        {
            return new List<Layout>();
        }

        var files = Directory.GetFiles(folderPath, "*.json");

        var layouts = files
            .AsParallel()
            .Select(file =>
            {
                try
                {
                    var fileLock = GetFileLock(file);
                    fileLock.Wait();
                    try
                    {
                        var layout = Json.Deserialize<Layout>(File.ReadAllText(file));
                        layout.Status = LayoutStatus.Saved;
                        return layout;
                    }
                    finally
                    {
                        fileLock.Release();
                    }
                }
                catch
                {
                    return null;
                }
            })
            .Where(layout => layout != null)
            .OrderBy(layout => layout?.Name)
            .ToList();

        return layouts;
    }

    private SemaphoreSlim GetFileLock(string filePath)
    {
        return _fileLocks.GetOrAdd(filePath, _ => new SemaphoreSlim(1, 1));
    }

    private string GetConfigPath<T>()
    {
        return Path.Combine(_rootFolder, $"{typeof(T).Name}.json");
    }

    private string GetLayoutPath(Layout layout)
    {
        return Path.Combine(_rootFolder, LayoutFolder, $"{layout.Guid}.json");
    }
}
