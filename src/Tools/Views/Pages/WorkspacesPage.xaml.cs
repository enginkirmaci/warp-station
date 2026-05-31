using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class WorkspacesPage : UserControl
{
    public WorkspacesViewModel ViewModel { get; }

    public WorkspacesPage()
    {
        InitializeComponent();
    }

    public WorkspacesPage(WorkspacesViewModel viewModel)
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
