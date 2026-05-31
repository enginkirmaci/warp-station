using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class FormattersPage : UserControl
{
    public FormattersPageViewModel ViewModel { get; }

    public FormattersPage()
    {
        InitializeComponent();
    }

    public FormattersPage(FormattersPageViewModel viewModel)
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
