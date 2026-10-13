using System.Collections.Concurrent;
using System.IO;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Services;

public class FileOperationService : IFileOperationService
{
	private const string LayoutFolder = "layouts";

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

				// One-time seed/migration: on first run (or the first run after an upgrade
				// from a version that stored SnapIt data inside the install dir), copy the
				// shipped defaults into the per-user folder so existing settings/layouts are
				// preserved. Files already present in the user folder are never overwritten.
				SeedFromShippedDefaults();

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

	/// <summary>
	/// Copies shipped default files from <see cref="Constants.InstallDefaultsFolder"/> into
	/// the per-user <see cref="Constants.RootFolder"/> when no user copy exists yet. Best
	/// effort: any error is swallowed so a missing/blocked default never blocks startup —
	/// <see cref="LoadAsync{T}"/> and <see cref="GetLayouts"/> synthesize empty defaults.
	/// </summary>
	private void SeedFromShippedDefaults()
	{
		var sourceFolder = Constants.InstallDefaultsFolder;
		if (!Directory.Exists(sourceFolder))
		{
			return;
		}

		try
		{
			// Seed top-level defaults (Settings.json, ExcludedApplicationSettings.json, ...).
			foreach (var sourceFile in Directory.GetFiles(sourceFolder, "*.json"))
			{
				var destFile = Path.Combine(_rootFolder, Path.GetFileName(sourceFile));
				if (!File.Exists(destFile))
				{
					File.Copy(sourceFile, destFile);
				}
			}

			// Seed layouts/*.json.
			var layoutsFolder = Path.Combine(_rootFolder, LayoutFolder);
			Directory.CreateDirectory(layoutsFolder);
			var sourceLayoutsFolder = Path.Combine(sourceFolder, LayoutFolder);
			if (Directory.Exists(sourceLayoutsFolder))
			{
				foreach (var sourceFile in Directory.GetFiles(sourceLayoutsFolder, "*.json"))
				{
					var destFile = Path.Combine(layoutsFolder, Path.GetFileName(sourceFile));
					if (!File.Exists(destFile))
					{
						File.Copy(sourceFile, destFile);
					}
				}
			}
		}
		catch
		{
			// Best-effort: fall through to per-file default creation on read.
		}
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