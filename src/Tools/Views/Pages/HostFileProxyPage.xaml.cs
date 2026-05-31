using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class HostFileProxyPage : UserControl
{
    public HostFileProxyViewModel ViewModel { get; }

    public HostFileProxyPage()
    {
        InitializeComponent();
    }

    public HostFileProxyPage(HostFileProxyViewModel viewModel)
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
