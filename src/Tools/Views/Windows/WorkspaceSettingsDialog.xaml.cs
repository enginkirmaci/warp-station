using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.Library.Configuration;
using Tools.ViewModels.Windows;

namespace Tools.Views.Windows;

/// <summary>
/// Modal dialog for editing workspace scan settings. A thin view: all editing state
/// and the array/string translation live in <see cref="WorkspaceSettingsViewModel"/>.
/// </summary>
public partial class WorkspaceSettingsDialog : Window
{
    /// <summary>
    /// Gets the ViewModel backing this dialog.
    /// </summary>
    public WorkspaceSettingsViewModel ViewModel { get; }

    /// <summary>
    /// Gets the edited workspace settings (valid after the user confirms via Save).
    /// </summary>
    public WorkspacesSettings Settings => ViewModel.BuildSettings();

    /// <summary>
    /// Initializes a new instance of the <see cref="WorkspaceSettingsDialog"/> class.
    /// </summary>
    /// <param name="currentSettings">The current workspace settings to edit.</param>
    public WorkspaceSettingsDialog(WorkspacesSettings currentSettings)
    {
        ViewModel = new WorkspaceSettingsViewModel(currentSettings);
        DataContext = ViewModel;
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// Handles the Save button click event.
    /// </summary>
    private void OnSaveClick(object? sender, RoutedEventArgs e)
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
