using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of settings service that manages application settings.
/// Acts as the single owner of the settings object graph: all reads return deep copies
/// so callers cannot mutate the in-memory cache out from under the service, all writes
/// are serialized through a lock, and the file is persisted atomically (temp file +
/// rename) so a crash mid-write cannot corrupt <c>settings.json</c>.
/// </summary>
public class SettingsService : ISettingsService
{
    private const string SettingsFileName = "settings.json";

    private static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };
    private static readonly JsonSerializerOptions WriteOptions = new() { WriteIndented = true };

    private readonly string _settingsFilePath;
    private readonly string _settingsDirectory;
    private readonly object _lock = new();
    private AppSettings? _cachedSettings;

    public SettingsService()
    {
        // BaseDirectory (where the executable lives) is reliable across hosts
        string baseDirectory = AppContext.BaseDirectory;
        _settingsDirectory = Path.Combine(baseDirectory, "settings");
        _settingsFilePath = Path.Combine(_settingsDirectory, SettingsFileName);
    }

    public Task<AppSettings> LoadSettingsAsync()
    {
        lock (_lock)
        {
            // Load the authoritative copy on demand, then hand back a copy.
            EnsureLoaded();
            return Task.FromResult(CopySettings(_cachedSettings!));
        }
    }

    public Task<AppSettings> GetSettingsAsync()
    {
        lock (_lock)
        {
            EnsureLoaded();
            return Task.FromResult(CopySettings(_cachedSettings!));
        }
    }

    public async Task SaveSettingsAsync(AppSettings settings)
    {
        // Adopt the caller's instance as the new authoritative state under the lock.
        var snapshot = settings;
        lock (_lock)
        {
            EnsureNestedSections(snapshot);
            _cachedSettings = snapshot;
        }

        try
        {
            await WriteAtomicallyAsync(snapshot);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error saving settings");
            throw;
        }
    }

    /// <summary>
    /// Loads the cached settings from disk if not already loaded. Must be called under
    /// <see cref="_lock"/>.
    /// </summary>
    private void EnsureLoaded()
    {
        if (_cachedSettings != null) return;

        try
        {
            if (File.Exists(_settingsFilePath))
            {
                var json = File.ReadAllText(_settingsFilePath);
                _cachedSettings = JsonSerializer.Deserialize<AppSettings>(json, ReadOptions) ?? new AppSettings();
            }
            else
            {
                _cachedSettings = new AppSettings();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error loading settings");
            _cachedSettings = new AppSettings();
        }

        EnsureNestedSections(_cachedSettings);
    }

    /// <summary>
    /// Guarantees nested sections are non-null so callers never hit NREs. Safe to call
    /// under the lock (no I/O).
    /// </summary>
    private static void EnsureNestedSections(AppSettings settings)
    {
        settings.EFToolsPage ??= new EFToolsPageSettings();
        settings.NugetLocal ??= new NugetLocalSettings();
        settings.Workspaces ??= new WorkspacesSettings();
        settings.ClipboardPassword ??= new ClipboardPasswordSettings();
        settings.SnapIt ??= new SnapItSettings();
        settings.General ??= new GeneralSettings();
    }

    /// <summary>
    /// Produces an isolated deep copy of <paramref name="source"/> via a JSON round-trip,
    /// so callers receive a snapshot they can mutate freely without affecting the cache.
    /// </summary>
    private static AppSettings CopySettings(AppSettings source)
    {
        var json = JsonSerializer.Serialize(source, WriteOptions);
        var copy = JsonSerializer.Deserialize<AppSettings>(json, ReadOptions)!;
        EnsureNestedSections(copy);
        return copy;
    }

    /// <summary>
    /// Writes the settings JSON atomically: serialize to a temp file in the same
    /// directory, then rename it over the target. A crash during the write leaves the
    /// previous file intact rather than a truncated/partial one.
    /// </summary>
    private async Task WriteAtomicallyAsync(AppSettings settings)
    {
        Directory.CreateDirectory(_settingsDirectory);

        var json = JsonSerializer.Serialize(settings, WriteOptions);
        var tempPath = _settingsFilePath + ".tmp";

        await File.WriteAllTextAsync(tempPath, json);

        // File.Move with overwrite is atomic on the same volume (POSIX rename / Win
        // ReplaceFile semantics), preventing a partial-write from corrupting the file.
        if (File.Exists(_settingsFilePath))
            File.Replace(tempPath, _settingsFilePath, destinationBackupFileName: null);
        else
            File.Move(tempPath, _settingsFilePath);
    }
}
