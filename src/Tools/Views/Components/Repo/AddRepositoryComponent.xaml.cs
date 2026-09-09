using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Platform.Storage;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components.Repo;

/// <summary>
/// Add Repositories component, hosted in the main window's floating tool drawer (the
/// former modal dialog). A thin view: scan and selection state live in
/// <see cref="AddRepositoryViewModel"/>, which receives its request through the drawer
/// context and resolves it with the selected paths on Add. Cancel/Add close the drawer
/// via the ViewModel; only the folder picker and the path box's Enter-to-scan stay here.
/// </summary>
public partial class AddRepositoryComponent : UserControl
{
    public AddRepositoryViewModel ViewModel { get; }

    public AddRepositoryComponent()
    {
        InitializeComponent();
    }

    public AddRepositoryComponent(AddRepositoryViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// Browse: opens the platform folder picker and scans the picked folder right away,
    /// so the usual flow is Browse → results without a separate Scan click. Failures
    /// (picker, scan) are logged — an async void handler would crash the process.
    /// </summary>
    private async void OnBrowseClick(object? sender, RoutedEventArgs e)
    {
        try
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
        catch (Exception ex)
        {
            Serilog.Log.Logger.Error(ex, "Add Repositories: browse-and-scan failed");
        }
    }

    /// <summary>Enter in the path box runs the scan; failures are logged, not thrown.</summary>
    private async void OnPathKeyDown(object? sender, KeyEventArgs e)
    {
        try
        {
            if (e.Key != Key.Enter) return;
            if (ViewModel.ScanCommand.CanExecute(null))
            {
                await ViewModel.ScanCommand.ExecuteAsync(null);
            }
            e.Handled = true;
        }
        catch (Exception ex)
        {
            Serilog.Log.Logger.Error(ex, "Add Repositories: scan failed");
        }
    }
}
