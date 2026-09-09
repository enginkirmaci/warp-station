using System.Linq;
using Avalonia.Controls;
using Avalonia.Controls.Presenters;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Media;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Tools.Library.Entities;
using Tools.ViewModels.Components;

namespace Tools.Views.Components;

/// <summary>
/// Bottom bar of the Repos page: tab strip (always visible once a repo is selected from
/// the table) and the active tab's expandable panel. Its DataContext is the singleton
/// <see cref="BottomBarViewModel"/> (attached by ReposPage). The branch ComboBox commits
/// its selection through a code-behind handler instead of a TwoWay binding so the
/// in-place branch reload never writes a transient null back into the view model; the
/// OpenCode settings drawer's model ComboBox does the same inside
/// <see cref="OpenCodeSettingsComponent"/>.
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

    /// <summary>
    /// A History row was tapped: open the commit-detail drawer on that commit. Taps on
    /// the row's hash Button are skipped — that button's own command (copy the full
    /// SHA) should not also open the drawer.
    /// </summary>
    private void OnHistoryRowTapped(object? sender, TappedEventArgs e)
    {
        for (StyledElement? source = e.Source as StyledElement; source is not null; source = source.Parent)
        {
            if (source is Button)
            {
                return;
            }
        }

        OpenHistoryRow(sender);
    }

    /// <summary>
    /// Keyboard path for a History row: the row Grid is focusable, Enter/Space act as
    /// a tap (an inner hash Button with focus handles its own key first, so its copy
    /// action keeps priority).
    /// </summary>
    private void OnHistoryRowKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key is not (Key.Enter or Key.Space))
        {
            return;
        }

        if (OpenHistoryRow(sender))
        {
            e.Handled = true;
        }
    }

    private bool OpenHistoryRow(object? sender)
    {
        if (sender is Control { DataContext: GitCommitInfo commit } && ViewModel is { } vm)
        {
            vm.OpenCommitDetailCommand.Execute(commit);
            return true;
        }

        return false;
    }

    /// <summary>
    /// Ctrl+Enter in the commit message box runs CommitCommand (plain Enter keeps its
    /// newline — the box is multi-line). The command's CanExecute (staged files, no
    /// in-flight commit) still gates it.
    /// </summary>
    private void OnCommitBoxKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key != Key.Enter || e.KeyModifiers != KeyModifiers.Control)
        {
            return;
        }

        if (ViewModel?.CommitCommand is { } commit && commit.CanExecute(null))
        {
            commit.Execute(null);
            e.Handled = true;
        }
    }
}
