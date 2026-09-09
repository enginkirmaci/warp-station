using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Azure DevOps tab of the bottom bar: pull requests, work items and pipeline
/// runs. Shares the bar's DataContext
/// (<see cref="Tools.ViewModels.Components.BottomBarViewModel"/>) and the shared
/// templates/styles in BottomBarResources.axaml.
/// </summary>
public partial class AzureTab : UserControl
{
    public AzureTab()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
