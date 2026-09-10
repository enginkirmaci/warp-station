using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components.Git;

/// <summary>
/// New Branch drawer component, hosted in the main window's floating tool drawer and
/// opened by the Changes tab's branch dropdown. A thin view: state and the git work
/// live in <see cref="NewBranchViewModel"/>, which receives the repo, the configured
/// branch-name prefix and the completion hook through the drawer context.
/// </summary>
public partial class NewBranchComponent : UserControl
{
    public NewBranchViewModel ViewModel { get; }

    /// <summary>Whether the prefix prefill's caret placement already ran — it must fire
    /// exactly once per open, on the context's first write into the name field.</summary>
    private bool _caretPlaced;

    public NewBranchComponent()
    {
        InitializeComponent();
    }

    public NewBranchComponent(NewBranchViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
    }

    /// <summary>
    /// Drops the caret at the end of the pre-typed prefix exactly once, so the user's
    /// first keystroke appends after it instead of inserting at position 0. Every later
    /// TextChanged (user typing) is ignored — mid-field edits must keep their caret.
    /// </summary>
    private void OnNameBoxTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (_caretPlaced || sender is not TextBox box || string.IsNullOrEmpty(box.Text)) return;

        _caretPlaced = true;
        box.CaretIndex = box.Text.Length;
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
