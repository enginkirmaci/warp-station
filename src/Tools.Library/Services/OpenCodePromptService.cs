using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-backed implementation of <see cref="IOpenCodePromptService"/>. Reads
/// <c>%USERPROFILE%\.devtools\opencode\prompts.json</c> (seeded into the user-data
/// folder on first run) and exposes the named prompts for the OpenCode panel selector.
/// </summary>
public class OpenCodePromptService : IOpenCodePromptService
{
    private static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };
    private static readonly JsonSerializerOptions WriteOptions = new() { WriteIndented = true };

    /// <summary>Shipped defaults live under <c>&lt;install&gt;/settings/opencode/prompts.json</c>.</summary>
    private const string ShippedRelPath = "opencode\\prompts.json";

    private readonly string _promptsFilePath;

    public OpenCodePromptService()
    {
        // User data under %USERPROFILE%\.devtools so it survives reinstalls. Seeded from
        // the shipped default on first run; user edits survive (seed-once, not refresh).
        _promptsFilePath = UserPaths.GetUserDataFile("opencode", "prompts.json");
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<OpenCodePromptEntry>> LoadAsync()
    {
        try
        {
            // Seed the default prompts only when the user has none yet: once a user copy
            // exists (theirs or previously seeded on first run), respect their setup
            // entirely and never clobber their edits.
            if (!File.Exists(_promptsFilePath))
                UserPaths.SeedFromDefault(_promptsFilePath, ShippedRelPath);

            if (!File.Exists(_promptsFilePath))
                return Array.Empty<OpenCodePromptEntry>();

            var json = await File.ReadAllTextAsync(_promptsFilePath);
            var config = JsonSerializer.Deserialize<OpenCodePromptConfig>(json, ReadOptions);
            if (config?.Prompts is null)
                return Array.Empty<OpenCodePromptEntry>();

            // Drop blanks (no name/prompt text) so the selector stays clean.
            var prompts = config.Prompts
                .Where(p => p is not null && !string.IsNullOrWhiteSpace(p.Name))
                .ToList();

            // Stable, human-friendly order: by name, case-insensitive.
            prompts.Sort((a, b) => string.Compare(a.Name, b.Name, StringComparison.OrdinalIgnoreCase));
            return prompts;
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error loading OpenCode prompts");
            return Array.Empty<OpenCodePromptEntry>();
        }
    }

    /// <inheritdoc/>
    public async Task SaveAsync(string name, string prompt)
    {
        // Treat blank names as a no-op: the selector drops them on load anyway, so
        // persisting one would be invisible.
        if (string.IsNullOrWhiteSpace(name))
            return;

        try
        {
            // Seed first so we never lose the shipped defaults when the user saves before
            // a load has run (e.g. on a fresh install).
            if (!File.Exists(_promptsFilePath))
                UserPaths.SeedFromDefault(_promptsFilePath, ShippedRelPath);

            var config = ReadConfig();

            // Replace any existing prompt with the same name (case-insensitive); otherwise
            // append. The (None) sentinel is never written to disk.
            var existing = config.Prompts.FirstOrDefault(p =>
                string.Equals(p.Name, name, StringComparison.OrdinalIgnoreCase));
            if (existing is not null)
            {
                existing.Prompt = prompt;
            }
            else
            {
                config.Prompts.Add(new OpenCodePromptEntry { Name = name, Prompt = prompt });
            }

            // Sort on write to keep the file stable and match LoadAsync's display order.
            config.Prompts.Sort((a, b) =>
                string.Compare(a.Name, b.Name, StringComparison.OrdinalIgnoreCase));

            var json = JsonSerializer.Serialize(config, WriteOptions);

            // Atomic write: serialize to a temp file then replace the target so a crash
            // mid-write never leaves a truncated prompts.json.
            var tempPath = _promptsFilePath + ".tmp";
            await File.WriteAllTextAsync(tempPath, json);
            if (File.Exists(_promptsFilePath))
                File.Replace(tempPath, _promptsFilePath, destinationBackupFileName: null);
            else
                File.Move(tempPath, _promptsFilePath);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error saving OpenCode prompt '{Name}'", name);
        }
    }

    /// <summary>
    /// Reads and deserializes the on-disk <c>prompts.json</c>. Returns an empty config
    /// when the file is missing or unreadable so a save can still proceed.
    /// </summary>
    private OpenCodePromptConfig ReadConfig()
    {
        if (!File.Exists(_promptsFilePath))
            return new OpenCodePromptConfig();

        var json = File.ReadAllText(_promptsFilePath);
        return JsonSerializer.Deserialize<OpenCodePromptConfig>(json, ReadOptions) ?? new OpenCodePromptConfig();
    }
}
