using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Components;

/// <summary>
/// Formatters tool (base64 / case conversions with history), hosted in the main
/// window's floating tool drawer.
/// </summary>
public partial class FormattersComponent : UserControl
{
    public FormattersViewModel ViewModel { get; }

    public FormattersComponent()
    {
        InitializeComponent();
    }

    public FormattersComponent(FormattersViewModel viewModel)
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
