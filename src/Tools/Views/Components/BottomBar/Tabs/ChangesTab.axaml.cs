using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Avalonia.Interactivity;
using Avalonia.Threading;
using Avalonia.VisualTree;
using Tools.Library.Services;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.Views.Components.BottomBar.Tabs;

/// <summary>
/// Changes tab of the bottom bar: staged/unstaged tree, commit workspace and the
/// git rail beside the message (branch dropdown, action menu, fetch status); the
/// History list itself is the shell-hosted ChangesHistoryView. Its
/// DataContext is the tab's <see cref="ChangesTabViewModel"/> (the bar shell's
/// child, assigned by <c>BottomBar.xaml</c>). The branch ComboBox commits its
/// selection through a code-behind handler instead of a TwoWay binding so the
/// in-place branch reload never writes a transient null back into the view model,
/// and its search row's TextBox does the same for the filter text (the row's
/// DataContext is the search record, not this tab).
/// </summary>
public partial class ChangesTab : UserControl
{
    public ChangesTabViewModel? ViewModel => DataContext as ChangesTabViewModel;

    private ComboBox? _branchPicker;

    public ChangesTab()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
        _branchPicker = this.FindControl<ComboBox>("BranchPicker");
    }

    /// <summary>
    /// Commits a dropdown pick: the ViewModel checks the branch out, or — for the New
    /// branch sentinel — opens the new-branch drawer and snaps the selection back (see
    /// <see cref="ChangesTabViewModel.OnSelectedMenuItemChanged"/>). Programmatic syncs
    /// re-commit the current branch and are ignored there. The search row is never
    /// committed: it is a filter, not a branch (the close path re-syncs the selection).
    /// </summary>
    private void OnBranchSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (e.AddedItems.Count > 0 && ViewModel is { } vm && e.AddedItems[0] is not BranchSearchEntry)
        {
            vm.SelectedMenuItem = e.AddedItems[0];
        }
    }

    /// <summary>
    /// The dropdown opened: park the caret in the search row so typing filters the
    /// branches straight away. The row's container exists only once the popup has laid
    /// out, so the focus lands on the next dispatcher pass.
    /// </summary>
    private void OnBranchPickerDropDownOpened(object? sender, EventArgs e)
    {
        if (_branchPicker is null) return;
        Dispatcher.UIThread.Post(() =>
        {
            if (_branchPicker.ContainerFromIndex(0) is ComboBoxItem row
                && row.GetVisualDescendants().OfType<TextBox>().FirstOrDefault() is { } search)
            {
                // The search row's container is reused across opens and the TextBox is
                // unbound (its DataContext is the row record) — mirror the view model's
                // (cleared) filter into it on every open. The container padding is
                // zeroed so the box spans the popup instead of sitting inset like the
                // selectable rows.
                row.Padding = new Thickness(0);
                search.Text = ViewModel?.BranchSearchText ?? string.Empty;
                search.Focus();
            }
        });
    }

    /// <summary>
    /// The dropdown closed: the view model drops the filter and re-asserts the current
    /// branch (items rebuilt mid-search leave the ComboBox's selection unresolved).
    /// </summary>
    private void OnBranchPickerDropDownClosed(object? sender, EventArgs e)
    {
        ViewModel?.OnBranchDropdownClosed();
    }

    /// <summary>
    /// Pushes the search row's text into the view model's filter. The compare skips
    /// programmatic echoes (the filter clears on close) so the rebuild doesn't churn.
    /// </summary>
    private void OnBranchSearchTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (sender is TextBox search && ViewModel is { } vm && search.Text != vm.BranchSearchText)
        {
            vm.BranchSearchText = search.Text;
        }
    }

    /// <summary>
    /// Enter with a search running checks out the first matching branch (menu order —
    /// locals before remotes); the ComboBox closes through the selection change. Escape
    /// clears the filter and closes here, in case the popup's own dismiss doesn't reach
    /// a TextBox-hosted key first.
    /// </summary>
    private void OnBranchSearchKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape)
        {
            if (sender is TextBox search)
            {
                search.Text = string.Empty;
            }
            if (_branchPicker is { } picker)
            {
                picker.IsDropDownOpen = false;
            }
            e.Handled = true;
            return;
        }

        if (e.Key != Key.Enter || ViewModel is not { } vm || vm.BranchSearchText.Trim().Length == 0)
        {
            return;
        }

        if (vm.FilteredBranchMenuItems.OfType<GitBranchRef>().FirstOrDefault(b => !b.IsHeader) is not { } first)
        {
            return;
        }

        if (_branchPicker is { } closeTarget)
        {
            closeTarget.IsDropDownOpen = false; // the close path clears the filter first…
        }
        vm.SelectedMenuItem = first;         // …then the checkout shows the target on the pill
        e.Handled = true;
    }

    /// <summary>
    /// Clicks inside the search row must reach the TextBox (caret, text selection)
    /// WITHOUT the ComboBoxItem committing the row — a committed search row closes the
    /// dropdown. The TextBox's own press/release work has already run by the time these
    /// instance handlers fire (class handlers precede them); marking the args handled
    /// only stops the bubbling toward the item container.
    /// </summary>
    private void OnBranchSearchPointerPressed(object? sender, PointerPressedEventArgs e) => e.Handled = true;

    private void OnBranchSearchPointerReleased(object? sender, PointerReleasedEventArgs e) => e.Handled = true;

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
