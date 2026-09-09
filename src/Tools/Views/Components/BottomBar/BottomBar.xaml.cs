using System.Linq;
using Avalonia.Controls;
using Avalonia.Controls.Presenters;
using Avalonia.Controls.Primitives;
using Avalonia.Markup.Xaml;
using Avalonia.VisualTree;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.Views.Components.BottomBar;

/// <summary>
/// Bottom bar of the Repos page: shell only — the drag divider, the expandable
/// panel card and the per-tab content controls under Tabs/. Its DataContext is
/// the singleton <see cref="BottomBarViewModel"/> (bound by ReposPage) and is
/// inherited by every tab control; the tab row lives in <c>RepoHeader</c> and
/// each tab's templates/styles come from <c>BottomBarResources.axaml</c>.
/// </summary>
public partial class BottomBar : UserControl
{
    public BottomBarViewModel? ViewModel => DataContext as BottomBarViewModel;

    public BottomBar()
    {
        InitializeComponent();
        ConstrainScrollViewersToViewportWidth();
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

    /// <summary>
    /// The bar's list ScrollViewers (tab lists, Overview preview cards) must measure
    /// their rows at the VIEWPORT width — otherwise long titles push the right-aligned
    /// age/pill columns out past the card edge instead of trimming in place. The
    /// HorizontalScrollBarVisibility=Disabled constraint is supposed to flow to the
    /// presenter, but the themed template wins that race on this setup, so the
    /// presenter's horizontal-scroll flag is re-asserted as a local value the moment
    /// each template applies (local beats template). Every tab is instantiated in the
    /// shell's XAML, so all of their ScrollViewers exist by ctor time.
    /// </summary>
    private void ConstrainScrollViewersToViewportWidth()
    {
        foreach (var scrollViewer in this.GetVisualDescendants().OfType<ScrollViewer>())
        {
            scrollViewer.TemplateApplied += OnRowScrollViewerTemplateApplied;
        }
    }

    private void OnRowScrollViewerTemplateApplied(object? sender, TemplateAppliedEventArgs e)
    {
        if (sender is not ScrollViewer scrollViewer)
        {
            return;
        }

        var presenter = e.NameScope.Find<ScrollContentPresenter>("PART_ContentPresenter")
            ?? scrollViewer.GetVisualDescendants().OfType<ScrollContentPresenter>().FirstOrDefault();
        if (presenter is not null)
        {
            presenter.CanHorizontallyScroll = false;
        }
    }
}
