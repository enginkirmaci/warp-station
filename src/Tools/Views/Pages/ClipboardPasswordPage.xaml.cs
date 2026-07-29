using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class ClipboardPasswordPage : UserControl
{
    public ClipboardPasswordViewModel ViewModel { get; }

    public ClipboardPasswordPage()
    {
        InitializeComponent();
    }

    public ClipboardPasswordPage(ClipboardPasswordViewModel viewModel)
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
