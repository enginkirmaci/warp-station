using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Reads the OpenCode templates shipped under <c>settings/opencode/templates/</c> and
/// copies a selected template into a repo as <c>.opencode</c> when launching.
/// </summary>
public interface IOpenCodeTemplateService
{
    /// <summary>
    /// Loads every discoverable template from the user-data folder, seeding from the
    /// shipped defaults first. Never throws: malformed entries are logged and skipped.
    /// </summary>
    Task<IReadOnlyList<OpenCodeTemplate>> LoadAsync();

    /// <summary>
    /// Copies the template's source folder into <c>&lt;repoFolderPath&gt;/.opencode</c>,
    /// deleting any existing <c>.opencode</c> first. A no-op when the template is the
    /// <see cref="OpenCodeTemplate.None"/> sentinel.
    /// </summary>
    Task CopyToRepoAsync(OpenCodeTemplate template, string repoFolderPath);
}
