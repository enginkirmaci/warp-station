using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class EFToolsPage : UserControl
{
    public EFToolsPageViewModel ViewModel { get; }

    public EFToolsPage()
    {
        InitializeComponent();
    }

    public EFToolsPage(EFToolsPageViewModel viewModel)
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
