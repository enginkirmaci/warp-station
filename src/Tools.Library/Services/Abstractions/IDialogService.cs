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
    /// Shows the modal workspace settings dialog for editing.
    /// </summary>
    /// <param name="current">The current workspace settings to edit.</param>
    /// <returns>
    /// The edited settings if the user confirmed, or <c>null</c> if the user cancelled.
    /// </returns>
    Task<WorkspacesSettings?> ShowWorkspaceSettingsDialogAsync(WorkspacesSettings current);
}
