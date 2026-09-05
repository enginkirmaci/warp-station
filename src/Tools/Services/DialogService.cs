using Avalonia.Controls;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using Tools.Helpers;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using Tools.Views.Windows;

namespace Tools.Services;

/// <summary>
/// Avalonia-backed implementation of <see cref="IDialogService"/>. Folder picking
/// delegates to the application's main window; the former modal dialogs (Repo Settings,
/// Add Repositories) are now components hosted in the main window's floating tool
/// drawer: this service opens the drawer on the component, hands it a context carrying
/// the request and a completion source, and awaits the completion. Closing the drawer
/// any other way — the header ✕, Cancel, the backdrop, Escape, or switching to another
/// tool — resolves as a cancel (null).
/// </summary>
public class DialogService : IDialogService
{
    private readonly MainWindow _mainWindow;
    private readonly IToolDrawerService _toolDrawer;

    public DialogService(MainWindow mainWindow, IToolDrawerService toolDrawer)
    {
        _mainWindow = mainWindow;
        _toolDrawer = toolDrawer;
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
    public Task<ReposSettings?> ShowReposSettingsDialogAsync(ReposSettings current)
    {
        var context = new ReposSettingsDrawerContext(current);
        return ShowDrawerComponentAsync(ToolComponentMapper.RepoSettingsKey, context, context.Completion);
    }

    /// <inheritdoc/>
    public Task<IReadOnlyList<string>?> ShowAddRepositoryDialogAsync(ReposSettings settings, IReadOnlyList<Repo> trackedRepos)
    {
        var context = new AddRepositoriesDrawerContext(settings, trackedRepos);
        return ShowDrawerComponentAsync(ToolComponentMapper.AddRepositoriesKey, context, context.Completion);
    }

    /// <summary>
    /// Opens the tool drawer on a dialog component and awaits its completion source. The
    /// component (through its ViewModel) resolves the source with the confirmed result;
    /// every other drawer close path — including a later <c>Open</c> on another tool —
    /// resolves it with <c>null</c> via the Changed subscription, so the awaiting caller
    /// always resumes. The result is marshaled back to the UI thread, matching the
    /// continuation semantics the former <c>ShowDialog</c> flow had.
    /// </summary>
    private async Task<T?> ShowDrawerComponentAsync<T>(string toolKey, object context, TaskCompletionSource<T?> completion)
    {
        // A drawer already showing this key would no-op the Open below and never deliver
        // the new context; the entry points sit under the drawer's backdrop, so this is
        // purely defensive — bail out as cancelled.
        if (_toolDrawer.IsOpen && _toolDrawer.SelectedToolKey == toolKey)
        {
            return default;
        }

        void OnDrawerChanged()
        {
            if (_toolDrawer.IsOpen && _toolDrawer.SelectedToolKey == toolKey)
            {
                return;
            }

            _toolDrawer.Changed -= OnDrawerChanged;
            completion.TrySetResult(default);
        }

        _toolDrawer.Changed += OnDrawerChanged;
        try
        {
            _toolDrawer.Open(toolKey, context);
            var result = await completion.Task;
            if (!Dispatcher.UIThread.CheckAccess())
            {
                result = await Dispatcher.UIThread.InvokeAsync(() => result);
            }
            return result;
        }
        finally
        {
            _toolDrawer.Changed -= OnDrawerChanged;
        }
    }
}
