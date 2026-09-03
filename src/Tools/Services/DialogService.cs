using Avalonia.Controls;
using Avalonia.Platform.Storage;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Views.Windows;

namespace Tools.Services;

/// <summary>
/// Avalonia-backed implementation of <see cref="IDialogService"/>. Delegates folder
/// picking and modal dialogs to the application's main window, so ViewModels stay free
/// of any direct dependency on the view layer.
/// </summary>
public class DialogService : IDialogService
{
    private readonly MainWindow _mainWindow;
    private readonly IGitHubService _gitHubService;
    private readonly IAzureDevOpsService _azureDevOpsService;
    private readonly IProcessLauncher _processLauncher;

    public DialogService(
        MainWindow mainWindow,
        IGitHubService gitHubService,
        IAzureDevOpsService azureDevOpsService,
        IProcessLauncher processLauncher)
    {
        _mainWindow = mainWindow;
        _gitHubService = gitHubService;
        _azureDevOpsService = azureDevOpsService;
        _processLauncher = processLauncher;
    }

    /// <inheritdoc/>
    public async Task<string?> PickFolderAsync(string title)
    {
        var topLevel = TopLevel.GetTopLevel(_mainWindow);
        if (topLevel == null)
            return null;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = title,
            AllowMultiple = false
        });

        return folders.Count > 0 ? folders[0].Path.LocalPath : null;
    }

    /// <inheritdoc/>
    public async Task<ReposSettings?> ShowReposSettingsDialogAsync(ReposSettings current)
    {
        var dialog = new ReposSettingsDialog(current);
        var confirmed = await dialog.ShowDialog<bool>(_mainWindow);
        return confirmed ? dialog.Settings : null;
    }

    /// <inheritdoc/>
    public async Task ShowGitHubDetailsDialogAsync(Repo repo)
    {
        var dialog = new GitHubDetailsDialog(repo, _gitHubService, _processLauncher);
        await dialog.ShowDialog(_mainWindow);
    }

    /// <inheritdoc/>
    public async Task ShowAzureDevOpsDetailsDialogAsync(Repo repo)
    {
        var dialog = new AzureDevOpsDetailsDialog(repo, _azureDevOpsService, _processLauncher);
        await dialog.ShowDialog(_mainWindow);
    }
}
