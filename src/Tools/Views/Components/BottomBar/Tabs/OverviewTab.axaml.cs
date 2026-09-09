using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Overview tab of the bottom bar: stat cards, activity preview lists and the
/// Repository Details sidebar. Shares the bar's DataContext
/// (<see cref="Tools.ViewModels.Components.BottomBar.BottomBarViewModel"/>) and the shared
/// templates/styles in BottomBarResources.axaml.
/// </summary>
public partial class OverviewTab : UserControl
{
    public OverviewTab()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
