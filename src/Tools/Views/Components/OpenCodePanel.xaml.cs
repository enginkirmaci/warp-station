using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Components;

namespace Tools.Views.Components;

/// <summary>
/// The OpenCode full bottom panel of the Repos page — what the repo row's options icon
/// opens instead of the old OpenCode tab: the bar's docked-card chrome with a simple
/// header (title + target repo + X) over the <see cref="OpenCodeComponent"/> launch
/// surface. Its DataContext is the singleton <see cref="BottomBarViewModel"/> (attached
/// by ReposPage, the same instance the BottomBar binds); visibility is the ViewModel's
/// <c>IsOpenCodePanelVisible</c> — opening the panel hides the bar and vice versa.
/// </summary>
public partial class OpenCodePanel : UserControl
{
    public BottomBarViewModel? ViewModel => DataContext as BottomBarViewModel;

    public OpenCodePanel()
    {
        InitializeComponent();
        PanelResizeController.Attach(
            this.FindControl<Border>("PanelResizer")
            ?? throw new InvalidOperationException("PanelResizer missing"),
            this,
            delta => ViewModel?.AdjustPanelHeight(delta));
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
