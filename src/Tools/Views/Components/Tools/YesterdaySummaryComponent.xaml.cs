using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Components.Tools;

/// <summary>
/// Yesterday's Summary tool (one opencode report across all repositories with
/// activity on the previous day), hosted in the main window's floating tool drawer.
/// </summary>
public partial class YesterdaySummaryComponent : UserControl
{
    public YesterdaySummaryViewModel ViewModel { get; }

    public YesterdaySummaryComponent()
    {
        InitializeComponent();
    }

    public YesterdaySummaryComponent(YesterdaySummaryViewModel viewModel)
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
