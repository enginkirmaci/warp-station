using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class ReposPage : UserControl
{
    public ReposViewModel ViewModel { get; }

    public ReposPage()
    {
        InitializeComponent();
    }

    public ReposPage(ReposViewModel viewModel)
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
