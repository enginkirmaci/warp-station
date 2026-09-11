using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components.History;

/// <summary>
/// Commit-detail view of the Changes tab's History list, hosted by the bottom bar
/// shell and opened by clicking a History row. A thin view: all state and git
/// actions live in <see cref="CommitHistoryViewModel"/>, which the hosting tab
/// view-model opens with the clicked commit.
/// </summary>
public partial class CommitHistoryComponent : UserControl
{
    public CommitHistoryComponent()
    {
        InitializeComponent();
    }

    /// <summary>
    /// The whole file row toggles its patch — the chevron is only a state indicator,
    /// far too small to be the sole hit target. Bound per row via Tapped on the row
    /// Grid (a wrapping Button would need a content-stretching ControlTheme to give a
    /// full-width hit area).
    /// </summary>
    private void OnFileRowTapped(object? sender, TappedEventArgs e)
    {
        ToggleFileRow(sender);
    }

    /// <summary>
    /// Keyboard path for a file row: the row Grid is focusable, Enter/Space act as a
    /// tap (Tapped covers pointer input only).
    /// </summary>
    private void OnFileRowKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key is not (Key.Enter or Key.Space))
        {
            return;
        }

        if (ToggleFileRow(sender))
        {
            e.Handled = true;
        }
    }

    private bool ToggleFileRow(object? sender)
    {
        if (sender is Control { DataContext: CommitFileRowViewModel row }
            && row.ToggleCommand.CanExecute(null))
        {
            row.ToggleCommand.Execute(null);
            return true;
        }

        return false;
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
