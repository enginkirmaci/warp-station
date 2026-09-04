using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Abstracts UI interactions (folder pickers, modal dialogs) so that ViewModels do not
/// depend on the application's <c>App.MainWindow</c> static or on Avalonia
/// <see cref="Avalonia.Controls.TopLevel"/> directly. This keeps ViewModels testable and
/// decoupled from the view layer.
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
    /// <returns>
    /// The edited settings if the user confirmed, or <c>null</c> if the user cancelled.
    /// </returns>
    Task<ReposSettings?> ShowReposSettingsDialogAsync(ReposSettings current);

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

    /// <summary>
    /// Shows the modal GitHub details dialog for a repo: its open pull requests (first)
    /// and issues (second) as clickable links, with a Refresh button re-running the
    /// <c>gh</c> fetch.
    /// </summary>
    /// <param name="repo">The repo whose GitHub activity is shown.</param>
    Task ShowGitHubDetailsDialogAsync(Entities.Repo repo);

    /// <summary>
    /// Shows the modal Azure DevOps details dialog for a repo: its active pull requests,
    /// the hosting project's open work items and the repo's recent pipeline runs as
    /// clickable links, with a Refresh button re-running the REST fetch.
    /// </summary>
    /// <param name="repo">The repo whose Azure DevOps activity is shown.</param>
    Task ShowAzureDevOpsDetailsDialogAsync(Entities.Repo repo);
}
