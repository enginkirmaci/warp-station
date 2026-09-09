using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components.Repo;

/// <summary>
/// Repo Settings component, hosted in the main window's floating tool drawer (the
/// former modal dialog). A thin view: all editing state and the array/string
/// translation live in <see cref="ReposSettingsViewModel"/>, which receives the
/// settings to edit through the drawer context and resolves the edited settings on Save.
/// </summary>
public partial class ReposSettingsComponent : UserControl
{
    public ReposSettingsViewModel ViewModel { get; }

    public ReposSettingsComponent()
    {
        InitializeComponent();
    }

    public ReposSettingsComponent(ReposSettingsViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
