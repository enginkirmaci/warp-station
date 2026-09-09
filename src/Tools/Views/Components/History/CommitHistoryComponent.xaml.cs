using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components.History;

/// <summary>
/// History commit-detail component, hosted in the main window's floating tool drawer
/// and opened by clicking a History row in the Changes tab. A thin view: all state and
/// git actions live in <see cref="CommitHistoryViewModel"/>, which receives the
/// clicked commit through the drawer context.
/// </summary>
public partial class CommitHistoryComponent : UserControl
{
    public CommitHistoryViewModel ViewModel { get; }

    public CommitHistoryComponent()
    {
        InitializeComponent();
    }

    public CommitHistoryComponent(CommitHistoryViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
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
