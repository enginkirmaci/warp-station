using System.IO;
using System.Linq;
using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-backed implementation of <see cref="IOpenCodeTemplateService"/>. Reads the
/// template folders shipped under <c>settings/opencode/templates/</c> (seeded into the
/// user-data folder on first run) and copies a selected template into a repo as
/// <c>.opencode</c> when launching OpenCode.
/// </summary>
public class OpenCodeTemplateService : IOpenCodeTemplateService
{
    private static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };

    private const string TemplatesFolderName = ".opencode";

    /// <summary>Shipped defaults live under <c>&lt;install&gt;/settings/opencode/templates</c>.</summary>
    private const string ShippedRelDirectory = "opencode\\templates";

    private readonly string _templatesDirectory;

    public OpenCodeTemplateService()
    {
        // User data under %USERPROFILE%\.devtools so it survives reinstalls. Seeded from
        // the shipped defaults on first run; user edits and user-added templates survive.
        _templatesDirectory = UserPaths.GetUserDataFile("settings", "opencode", "templates");
    }

    /// <inheritdoc/>
    public Task<IReadOnlyList<OpenCodeTemplate>> LoadAsync()
    {
        try
        {
            // Seed the predefined templates only when the user has none yet: once the user
            // has any templates (theirs or previously seeded on first run), respect their
            // setup entirely and never introduce new predefined ones into a customized folder.
            var hasUserTemplates = Directory.Exists(_templatesDirectory) &&
                                   Directory.EnumerateDirectories(_templatesDirectory).Any();
            if (!hasUserTemplates)
                UserPaths.SeedDirectoryFromDefault(_templatesDirectory, ShippedRelDirectory);

            if (!Directory.Exists(_templatesDirectory))
                return Task.FromResult<IReadOnlyList<OpenCodeTemplate>>(Array.Empty<OpenCodeTemplate>());

            var templates = new List<OpenCodeTemplate>();
            foreach (var dir in Directory.EnumerateDirectories(_templatesDirectory))
            {
                var template = LoadTemplate(dir);
                if (template is not null)
                    templates.Add(template);
            }

            // Stable, human-friendly order: by name, case-insensitive.
            templates.Sort((a, b) => string.Compare(a.Name, b.Name, StringComparison.OrdinalIgnoreCase));
            return Task.FromResult<IReadOnlyList<OpenCodeTemplate>>(templates);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error loading OpenCode templates");
            return Task.FromResult<IReadOnlyList<OpenCodeTemplate>>(Array.Empty<OpenCodeTemplate>());
        }
    }

    /// <inheritdoc/>
    public Task CopyToRepoAsync(OpenCodeTemplate template, string repoFolderPath)
    {
        if (template.IsNone || string.IsNullOrWhiteSpace(repoFolderPath))
            return Task.CompletedTask;

        try
        {
            var destination = Path.Combine(repoFolderPath, TemplatesFolderName);

            // Replace any existing .opencode wholesale: delete first, then copy.
            if (Directory.Exists(destination))
                Directory.Delete(destination, recursive: true);

            CopyDirectory(template.FolderPath, destination);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error copying OpenCode template '{Name}' to '{Repo}'", template.Name, repoFolderPath);
        }

        return Task.CompletedTask;
    }

    /// <summary>
    /// Reads <c>template.json</c> from <paramref name="folder"/>. Returns null (and logs)
    /// when the folder has no manifest, or the manifest is unreadable/invalid. The name
    /// falls back to the folder name when blank.
    /// </summary>
    private static OpenCodeTemplate? LoadTemplate(string folder)
    {
        try
        {
            var manifestPath = Path.Combine(folder, "template.json");
            if (!File.Exists(manifestPath))
            {
                Log.Logger.Warning("Skipping OpenCode template folder without template.json: {Folder}", folder);
                return null;
            }

            var json = File.ReadAllText(manifestPath);
            var config = JsonSerializer.Deserialize<OpenCodeTemplateConfig>(json, ReadOptions);
            if (config is null)
                return null;

            var folderName = Path.GetFileName(folder);
            return new OpenCodeTemplate
            {
                Name = string.IsNullOrWhiteSpace(config.Name) ? folderName : config.Name,
                Description = config.Description ?? string.Empty,
                FolderPath = folder,
            };
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error reading OpenCode template manifest in {Folder}", folder);
            return null;
        }
    }

    /// <summary>Recursively copies a directory tree (contents included).</summary>
    private static void CopyDirectory(string source, string destination)
    {
        Directory.CreateDirectory(destination);
        foreach (var file in Directory.EnumerateFiles(source))
        {
            File.Copy(file, Path.Combine(destination, Path.GetFileName(file)), overwrite: true);
        }
        foreach (var dir in Directory.EnumerateDirectories(source))
        {
            CopyDirectory(dir, Path.Combine(destination, Path.GetFileName(dir)));
        }
    }
}
