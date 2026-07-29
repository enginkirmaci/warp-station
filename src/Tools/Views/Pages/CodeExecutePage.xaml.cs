using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class CodeExecutePage : UserControl
{
    public CodeExecuteViewModel ViewModel { get; }

    public CodeExecutePage()
    {
        InitializeComponent();
    }

    public CodeExecutePage(CodeExecuteViewModel viewModel)
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
