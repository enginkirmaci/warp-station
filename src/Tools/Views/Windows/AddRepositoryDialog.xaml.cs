using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Platform.Storage;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Windows;

namespace Tools.Views.Windows;

/// <summary>
/// Modal dialog for adding repositories by scanning a folder: the user picks (Browse)
/// or types a path, the scan lists the git repositories found underneath it, and the
/// checked ones are returned to the caller to be tracked. A thin view: scan and
/// selection state live in <see cref="AddRepositoryViewModel"/>.
/// </summary>
public partial class AddRepositoryDialog : Window
{
    /// <summary>
    /// Gets the ViewModel backing this dialog.
    /// </summary>
    public AddRepositoryViewModel ViewModel { get; }

    /// <summary>
    /// Initializes a new instance of the Add Repositories dialog.
    /// </summary>
    /// <param name="settings">
    /// The repo scan settings, read for their exclusions, folder pattern and scan depth.
    /// </param>
    /// <param name="trackedRepos">The currently tracked repos (their rows show "Already added").</param>
    /// <param name="scanner">The scanner that runs the folder walk.</param>
    public AddRepositoryDialog(ReposSettings settings, IReadOnlyList<Repo> trackedRepos, IRepoScanner scanner)
    {
        ViewModel = new AddRepositoryViewModel(settings, trackedRepos, scanner);
        DataContext = ViewModel;
        InitializeComponent();
    }

    /// <summary>
    /// Designer/XAML-compiler constructor. Not used at runtime — the app always
    /// constructs the dialog with its dependencies.
    /// </summary>
    public AddRepositoryDialog()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// Gets the checked, not-yet-tracked repo paths (valid after the user confirms via Add).
    /// </summary>
    public IReadOnlyList<string> GetSelectedPaths() => ViewModel.GetSelectedPaths();

    /// <summary>
    /// Browse: opens the platform folder picker and scans the picked folder right away,
    /// so the usual flow is Browse → results without a separate Scan click.
    /// </summary>
    private async void OnBrowseClick(object? sender, RoutedEventArgs e)
    {
        var topLevel = TopLevel.GetTopLevel(this);
        if (topLevel is null) return;

        var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Choose a folder to scan for repositories",
            AllowMultiple = false
        });
        if (folders.Count == 0) return;

        ViewModel.FolderPath = folders[0].Path.LocalPath;
        if (ViewModel.ScanCommand.CanExecute(null))
        {
            await ViewModel.ScanCommand.ExecuteAsync(null);
        }
    }

    /// <summary>Enter in the path box runs the scan.</summary>
    private async void OnPathKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key != Key.Enter) return;
        if (ViewModel.ScanCommand.CanExecute(null))
        {
            await ViewModel.ScanCommand.ExecuteAsync(null);
        }
        e.Handled = true;
    }

    /// <summary>
    /// Handles the Add button click event.
    /// </summary>
    private void OnAddClick(object? sender, RoutedEventArgs e)
    {
        Close(true);
    }

    /// <summary>
    /// Handles the Cancel button click event.
    /// </summary>
    private void OnCancelClick(object? sender, RoutedEventArgs e)
    {
        Close(false);
    }
}
