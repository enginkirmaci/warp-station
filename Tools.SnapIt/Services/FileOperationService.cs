using System.Collections.Concurrent;
using System.IO;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Services.Abstractions;

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
			var json = Helpers.Json.Serialize(config);
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
				var defaultJson = Helpers.Json.Serialize(new T());
				await File.WriteAllTextAsync(configPath, defaultJson);
				return new T();
			}

			var json = await File.ReadAllTextAsync(configPath);
			return Helpers.Json.Deserialize<T>(json);
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
		var layouts = new List<Layout>();

		foreach (var file in files)
		{
			var fileLock = GetFileLock(file);
			fileLock.Wait();
			try
			{
				var layout = Helpers.Json.Deserialize<Layout>(File.ReadAllText(file));
				if (layout != null)
				{
					layout.Status = LayoutStatus.Saved;
					layouts.Add(layout);
				}
			}
			catch
			{
			}
			finally
			{
				fileLock.Release();
			}
		}

		layouts.Sort((a, b) => string.Compare(a?.Name, b?.Name, StringComparison.Ordinal));
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

}