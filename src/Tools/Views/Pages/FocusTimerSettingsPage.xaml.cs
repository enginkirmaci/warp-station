using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class FocusTimerSettingsPage : UserControl
{
    public FocusTimerSettingsViewModel ViewModel { get; }

    public FocusTimerSettingsPage()
    {
        InitializeComponent();
    }

    public FocusTimerSettingsPage(FocusTimerSettingsViewModel viewModel)
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
