using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Avalonia.Interactivity;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Changes tab of the bottom bar: staged/unstaged tree, commit workspace and the
/// git rail beside the message (branch dropdown, action menu, fetch status); the
/// History list itself is the shell-hosted ChangesHistoryView. Its
/// DataContext is the tab's <see cref="ChangesTabViewModel"/> (the bar shell's
/// child, assigned by <c>BottomBar.xaml</c>). The branch ComboBox commits its
/// selection through a code-behind handler instead of a TwoWay binding so the
/// in-place branch reload never writes a transient null back into the view model.
/// </summary>
public partial class ChangesTab : UserControl
{
    public ChangesTabViewModel? ViewModel => DataContext as ChangesTabViewModel;

    public ChangesTab()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// Commits a dropdown pick: the ViewModel checks the branch out, or — for the New
    /// branch sentinel — opens the new-branch drawer and snaps the selection back (see
    /// <see cref="ChangesTabViewModel.OnSelectedMenuItemChanged"/>). Programmatic syncs
    /// re-commit the current branch and are ignored there.
    /// </summary>
    private void OnBranchSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (e.AddedItems.Count > 0 && ViewModel is { } vm)
        {
            vm.SelectedMenuItem = e.AddedItems[0];
        }
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
