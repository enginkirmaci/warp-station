using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Pull Requests tab of the bottom bar: GitHub's open pull requests plus the
/// Azure DevOps pull requests section. Shares the bar's DataContext
/// (<see cref="Tools.ViewModels.Components.BottomBar.BottomBarViewModel"/>) and the shared
/// templates/styles in BottomBarResources.axaml.
/// </summary>
public partial class PullRequestsTab : UserControl
{
    public PullRequestsTab()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
