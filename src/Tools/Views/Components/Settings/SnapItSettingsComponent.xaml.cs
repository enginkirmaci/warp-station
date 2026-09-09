using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Components.Settings;

/// <summary>
/// SnapIt settings tool (auto-start, running state, start/stop), hosted in the main
/// window's floating tool drawer.
/// </summary>
public partial class SnapItSettingsComponent : UserControl
{
    public SnapItSettingsViewModel ViewModel { get; }

    public SnapItSettingsComponent()
    {
        InitializeComponent();
    }

    public SnapItSettingsComponent(SnapItSettingsViewModel viewModel)
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
