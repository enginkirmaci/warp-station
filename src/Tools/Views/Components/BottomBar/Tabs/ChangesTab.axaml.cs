using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Avalonia.Interactivity;
using Tools.Library.Entities;
using Tools.ViewModels.Components;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Changes tab of the bottom bar: branch toolbar, staged/unstaged tree, commit
/// workspace and the collapsed History list. Its DataContext is the bar's
/// <see cref="BottomBarViewModel"/>. The branch ComboBox commits its selection
/// through a code-behind handler instead of a TwoWay binding so the in-place
/// branch reload never writes a transient null back into the view model.
/// </summary>
public partial class ChangesTab : UserControl
{
    public BottomBarViewModel? ViewModel => DataContext as BottomBarViewModel;

    public ChangesTab()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
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
