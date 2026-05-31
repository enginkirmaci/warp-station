using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class SnapItSettingsPage : UserControl
{
    public SnapItSettingsViewModel ViewModel { get; }

    public SnapItSettingsPage()
    {
        InitializeComponent();
    }

    public SnapItSettingsPage(SnapItSettingsViewModel viewModel)
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
