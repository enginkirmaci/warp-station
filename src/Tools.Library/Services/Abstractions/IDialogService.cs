using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// The Repo Settings drawer's confirmed result: the edited Repos section, the OpenCode
/// model fields (moved there from the OpenCode drawer — the record carries only those;
/// the caller merges them into the stored OpenCode section so flags the dialog doesn't
/// show keep their values) and the NuGet enable flag (same story — merged into the
/// stored NugetLocal section). One composite so the caller persists everything in a
/// single <c>SaveSettingsAsync</c> — two separate saves on a pre-dialog snapshot would
/// let the second overwrite the first's section.
/// </summary>
public sealed record ReposSettingsEditResult(ReposSettings Repos, OpenCodeSettings OpenCode, bool EnableNuget);

/// <summary>
/// Abstracts UI interactions (folder pickers, repo settings and Add Repositories flows)
/// so that ViewModels do not depend on the application's <c>App.MainWindow</c> static or
/// on Avalonia <see cref="Avalonia.Controls.TopLevel"/> directly. The two flows are
/// shown as components in the main window's floating tool drawer; both keep dialog
/// return semantics: the confirmed result, or <c>null</c> when the user cancelled.
/// </summary>
public interface IDialogService
{
    /// <summary>
    /// Shows a folder picker dialog and returns the selected folder path, or
    /// <c>null</c> if the user cancelled.
    /// </summary>
    /// <param name="title">The title of the folder picker dialog.</param>
    /// <returns>The selected folder path, or <c>null</c>.</returns>
    Task<string?> PickFolderAsync(string title);

    /// <summary>
    /// Shows the modal repo settings dialog for editing.
    /// </summary>
    /// <param name="current">The current repo settings to edit.</param>
    /// <param name="openCode">The current OpenCode settings (models) to edit.</param>
    /// <param name="nugetEnabled">The current NuGet enable flag to edit.</param>
    /// <returns>
    /// The edited sections if the user confirmed, or <c>null</c> if the user cancelled.
    /// </returns>
    Task<ReposSettingsEditResult?> ShowReposSettingsDialogAsync(ReposSettings current, OpenCodeSettings openCode, bool nugetEnabled);

    /// <summary>
    /// Shows the modal Add Repositories dialog: the user picks or types a folder, the
    /// folder is scanned for git repositories, and the checked findings are returned.
    /// </summary>
    /// <param name="settings">
    /// The repo scan settings, read by the dialog for their exclusions, folder pattern
    /// and scan depth.
    /// </param>
    /// <param name="trackedRepos">The currently tracked repos; their findings show as
    /// "Already added" and cannot be re-added.</param>
    /// <returns>
    /// The selected repo folder paths if the user confirmed, or <c>null</c> if the user
    /// cancelled (an empty list means confirmed with nothing to add).
    /// </returns>
    Task<IReadOnlyList<string>?> ShowAddRepositoryDialogAsync(ReposSettings settings, IReadOnlyList<Entities.Repo> trackedRepos);
}
