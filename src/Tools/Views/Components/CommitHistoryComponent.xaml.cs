using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components;

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

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
