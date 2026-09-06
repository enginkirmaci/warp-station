using System.Linq;
using Avalonia.Controls;
using Avalonia.Controls.Presenters;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.VisualTree;
using Tools.ViewModels.Components;

namespace Tools.Views.Components;

/// <summary>
/// Bottom bar of the Repos page: tab strip (always visible once a repo is selected from
/// the table) and the active tab's expandable panel. Its DataContext is the singleton
/// <see cref="BottomBarViewModel"/> (attached by ReposPage). The branch ComboBox commits
/// its selection through a code-behind handler instead of a TwoWay binding so the
/// in-place branch reload never writes a transient null back into the view model; the
/// OpenCode panel's model ComboBox does the same inside <see cref="OpenCodeComponent"/>.
/// </summary>
public partial class BottomBar : UserControl
{
    public BottomBarViewModel? ViewModel => DataContext as BottomBarViewModel;

    public BottomBar()
    {
        InitializeComponent();
        ConstrainScrollViewersToViewportWidth();
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
    /// each template applies (local beats template).
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
        Serilog.Log.Logger.Debug(
            "BottomBar ScrollViewer diag: hsb={Hsb} presenter={Found} presenterCanH={CanH} svW={W} presenterW={PW}",
            scrollViewer.HorizontalScrollBarVisibility,
            presenter is not null,
            presenter?.CanHorizontallyScroll,
            scrollViewer.Bounds.Width,
            presenter?.Bounds.Width);
    }

    /// <summary>
    /// Commits a branch pick: the ViewModel checks out the branch (see
    /// <see cref="BottomBarViewModel.OnSelectedBranchChanged"/>). Programmatic syncs
    /// re-commit the current branch and are ignored there.
    /// </summary>
    private void OnBranchSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (e.AddedItems.Count > 0 && e.AddedItems[0] is string branch && ViewModel is { } vm)
        {
            vm.SelectedBranch = branch;
        }
    }

    // --- Panel-top drag divider: resizes the expanded panel (per-tab height) ---

    private bool _resizingPanel;
    private double _lastDragY;

    private void OnPanelResizerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is not Border border) return;
        _resizingPanel = true;
        // Fixed reference (this control doesn't move during the drag), so the raw
        // delta is the pointer's travel.
        _lastDragY = e.GetPosition(this).Y;
        e.Pointer.Capture(border);
        e.Handled = true;
    }

    private void OnPanelResizerMoved(object? sender, PointerEventArgs e)
    {
        if (!_resizingPanel || sender is Border { } border && !e.GetCurrentPoint(border).Properties.IsLeftButtonPressed)
        {
            _resizingPanel = false;
            return;
        }

        var y = e.GetPosition(this).Y;
        ViewModel?.AdjustPanelHeight(_lastDragY - y); // dragging up grows the panel
        _lastDragY = y;
        e.Handled = true;
    }

    private void OnPanelResizerReleased(object? sender, PointerReleasedEventArgs e)
    {
        _resizingPanel = false;
        if (sender is Border border)
        {
            e.Pointer.Capture(null);
        }

        e.Handled = true;
    }
}
