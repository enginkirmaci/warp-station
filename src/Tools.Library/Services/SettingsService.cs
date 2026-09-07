using System.Text.Json;
using System.Text.Json.Nodes;
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
    // Cached JSON of the authoritative _cachedSettings. CopySettings now only
    // deserializes from this string instead of serialize+deserialize per read,
    // so repeated GetSettingsAsync() calls (one per page navigation) skip the
    // serialize half and avoid re-allocating the JSON string each time.
    private string? _cachedSettingsJson;

    public SettingsService()
    {
        // User settings live under %USERPROFILE%\.devtools so they survive reinstalls.
        // The shipped <install>/settings/settings.json is used only as a one-time seed.
        _settingsDirectory = UserPaths.UserDataRoot;
        _settingsFilePath = Path.Combine(_settingsDirectory, SettingsFileName);
    }

    public Task<AppSettings> LoadSettingsAsync()
    {
        lock (_lock)
        {
            // Load the authoritative copy on demand, then hand back a copy.
            EnsureLoaded();
            return Task.FromResult(CopySettings());
        }
    }

    public Task<AppSettings> GetSettingsAsync()
    {
        lock (_lock)
        {
            EnsureLoaded();
            return Task.FromResult(CopySettings());
        }
    }

    public async Task SaveSettingsAsync(AppSettings settings)
    {
        // Adopt the caller's instance as the new authoritative state under the lock.
        var snapshot = settings;
        string json;
        lock (_lock)
        {
            EnsureNestedSections(snapshot);
            _cachedSettings = snapshot;
            // Serialize once under the lock: it's needed both for the atomic
            // write and for the read cache, and computing it here keeps them in
            // sync so subsequent reads skip the serialize half.
            json = JsonSerializer.Serialize(snapshot, WriteOptions);
            _cachedSettingsJson = json;
        }

        try
        {
            await WriteAtomicallyAsync(json);
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
            // One-time seed/migration: copy the shipped default into the user folder on
            // first run (or the first run after upgrading from an in-install-dir version).
            UserPaths.SeedFromDefault(_settingsFilePath, SettingsFileName);

            if (File.Exists(_settingsFilePath))
            {
                var json = File.ReadAllText(_settingsFilePath);
                json = MigrateLegacyEnableKeys(json);
                _cachedSettings = JsonSerializer.Deserialize<AppSettings>(json, ReadOptions) ?? new AppSettings();
                _cachedSettingsJson = json;
            }
            else
            {
                _cachedSettings = new AppSettings();
                _cachedSettingsJson = JsonSerializer.Serialize(_cachedSettings, WriteOptions);
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error loading settings");
            _cachedSettings = new AppSettings();
            _cachedSettingsJson = JsonSerializer.Serialize(_cachedSettings, WriteOptions);
        }

        EnsureNestedSections(_cachedSettings);
    }

    /// <summary>
    /// Guarantees nested sections are non-null so callers never hit NREs. Safe to call
    /// under the lock (no I/O).
    /// </summary>
    private static void EnsureNestedSections(AppSettings settings)
    {
        settings.NugetLocal ??= new NugetLocalSettings();
        settings.Repos ??= new ReposSettings();
        settings.ClipboardPassword ??= new ClipboardPasswordSettings();
        settings.SnapIt ??= new SnapItSettings();
        settings.OpenCode ??= new OpenCodeSettings();
        settings.General ??= new GeneralSettings();
    }

    /// <summary>
    /// Maps the legacy per-tool toggle keys to the uniform <c>Enable&lt;Tool&gt;</c>
    /// scheme (2026-09): <c>Repos.ShowGitHubColumn</c> → <c>Repos.EnableGitHub</c>,
    /// <c>Repos.ShowAzureDevOpsColumn</c> → <c>Repos.EnableAzureDevOps</c>,
    /// <c>OpenCode.Enabled</c> → <c>OpenCode.EnableOpenCode</c>, and the INVERTED
    /// <c>ClipboardPassword.HideFromGui</c> → <c>ClipboardPassword.EnableClipboardPassword</c>
    /// (!). Each mapping applies only when the new key is absent from the file, so a file
    /// already carrying the new names (or a deliberate override) is untouched. Legacy
    /// keys stay in the file until the next save rewrites it from the model. Runs on the
    /// raw JSON so the parse cache and the model stay consistent; never throws.
    /// </summary>
    private static string MigrateLegacyEnableKeys(string json)
    {
        try
        {
            var node = JsonNode.Parse(json);
            if (node is null)
            {
                return json;
            }

            CopyLegacyBool(node["Repos"], "ShowGitHubColumn", "EnableGitHub", inverted: false);
            CopyLegacyBool(node["Repos"], "ShowAzureDevOpsColumn", "EnableAzureDevOps", inverted: false);
            CopyLegacyBool(node["OpenCode"], "Enabled", "EnableOpenCode", inverted: false);
            CopyLegacyBool(node["ClipboardPassword"], "HideFromGui", "EnableClipboardPassword", inverted: true);
            return node.ToJsonString();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "settings.json legacy enable-key migration failed; continuing with the file as-is");
            return json;
        }
    }

    /// <summary>
    /// Copies a legacy bool to its new name (optionally inverted), unless the new key
    /// already exists. Does nothing when the section or the legacy key is missing.
    /// </summary>
    private static void CopyLegacyBool(JsonNode? section, string legacyName, string newName, bool inverted)
    {
        var legacy = section?[legacyName];
        if (legacy is null || section![newName] is not null)
        {
            return;
        }

        var value = legacy.GetValue<bool>();
        section[newName] = inverted ? !value : value;
    }

    /// <summary>
    /// Produces an isolated deep copy of the cached settings by deserializing
    /// the cached JSON snapshot, so callers receive a snapshot they can mutate
    /// freely without affecting the cache. The serialize half is paid only when
    /// the authoritative state changes (load/save), not on every read.
    /// </summary>
    private AppSettings CopySettings()
    {
        var copy = JsonSerializer.Deserialize<AppSettings>(_cachedSettingsJson!, ReadOptions)!;
        EnsureNestedSections(copy);
        return copy;
    }

    /// <summary>
    /// Writes the already-serialized settings JSON atomically: write to a temp
    /// file in the same directory, then rename it over the target. A crash
    /// during the write leaves the previous file intact rather than a
    /// truncated/partial one.
    /// </summary>
    private async Task WriteAtomicallyAsync(string json)
    {
        Directory.CreateDirectory(_settingsDirectory);

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
