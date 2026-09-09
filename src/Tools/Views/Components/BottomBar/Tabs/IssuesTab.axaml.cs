using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Issues tab of the bottom bar: GitHub's open issues plus the Azure DevOps work
/// items section. Shares the bar's DataContext
/// (<see cref="Tools.ViewModels.Components.BottomBar.BottomBarViewModel"/>) and the shared
/// templates/styles in BottomBarResources.axaml.
/// </summary>
public partial class IssuesTab : UserControl
{
    public IssuesTab()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
