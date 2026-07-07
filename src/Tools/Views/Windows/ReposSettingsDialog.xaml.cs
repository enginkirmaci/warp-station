using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.Library.Configuration;
using Tools.ViewModels.Windows;

namespace Tools.Views.Windows;

/// <summary>
/// Modal dialog for editing repo scan settings. A thin view: all editing state
/// and the array/string translation live in <see cref="ReposSettingsViewModel"/>.
/// </summary>
public partial class ReposSettingsDialog : Window
{
    /// <summary>
    /// Gets the ViewModel backing this dialog.
    /// </summary>
    public ReposSettingsViewModel ViewModel { get; }

    /// <summary>
    /// Gets the edited repo settings (valid after the user confirms via Save).
    /// </summary>
    public ReposSettings Settings => ViewModel.BuildSettings();

    /// <summary>
    /// Initializes a new instance of the <see cref="ReposSettingsDialog"/> class.
    /// </summary>
    /// <param name="currentSettings">The current repo settings to edit.</param>
    public ReposSettingsDialog(ReposSettings currentSettings)
    {
        ViewModel = new ReposSettingsViewModel(currentSettings);
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
