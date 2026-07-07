using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class NugetLocalPage : UserControl
{
    public NugetLocalViewModel ViewModel { get; }

    public NugetLocalPage()
    {
        InitializeComponent();
    }

    public NugetLocalPage(NugetLocalViewModel viewModel)
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
