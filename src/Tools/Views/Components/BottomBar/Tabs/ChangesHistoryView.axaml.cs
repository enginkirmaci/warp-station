using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Avalonia.Interactivity;
using Tools.Library.Entities;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Full-width History view of the Changes tab (hosted by the bar shell, covering
/// the tab row and content). Its DataContext is the tab's
/// <see cref="ChangesTabViewModel"/>; the view only renders the recent-commits
/// list — opening, closing and loading live on the view-model.
/// </summary>
public partial class ChangesHistoryView : UserControl
{
    public ChangesHistoryView()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
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
        if (sender is Control { DataContext: GitCommitInfo commit } && DataContext is ChangesTabViewModel vm)
        {
            vm.OpenCommitDetailCommand.Execute(commit);
            return true;
        }

        return false;
    }
}
