using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class ClipboardPasswordPage : UserControl
{
    public ClipboardPasswordPageViewModel ViewModel { get; }

    public ClipboardPasswordPage()
    {
        InitializeComponent();
    }

    public ClipboardPasswordPage(ClipboardPasswordPageViewModel viewModel)
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
