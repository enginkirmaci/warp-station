using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// A confirmed settings edit: the edited Repos section, the edited OpenCode section
/// and the NuGet enable flag. One composite so the caller applies everything together
/// (live surfaces + one settings write) instead of section by section.
/// </summary>
public sealed record ReposSettingsEditResult(ReposSettings Repos, OpenCodeSettings OpenCode, bool EnableNuget);

/// <summary>
/// Abstracts UI interactions (folder pickers and the Add Repositories flow)
/// so that ViewModels do not depend on the application's <c>App.MainWindow</c> static or
/// on Avalonia <see cref="Avalonia.Controls.TopLevel"/> directly. The dialog flow is
/// shown as a component in the main window's floating tool drawer; it keeps dialog
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
