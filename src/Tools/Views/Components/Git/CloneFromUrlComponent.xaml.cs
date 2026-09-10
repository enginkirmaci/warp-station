using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.Library.Services;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components.Git;

/// <summary>
/// Clone from URL drawer component, hosted in the main window's floating tool drawer
/// and opened by the repos page toolbar. A thin view: state and the git work live in
/// <see cref="CloneFromUrlViewModel"/>; this class only wires the URL to folder-name
/// auto-derive, which stops the moment the user claims the name field.
/// </summary>
public partial class CloneFromUrlComponent : UserControl
{
    public CloneFromUrlViewModel ViewModel { get; }

    /// <summary>Whether the user has edited the folder-name field by hand — from that
    /// keystroke on, URL edits no longer overwrite their text.</summary>
    private bool _nameEdited;

    /// <summary>The last value the auto-derive wrote into the name field. The name
    /// box's TextChanged identifies our own writes by comparing against this — a
    /// busy-flag around the programmatic write is not enough, because the event can
    /// surface for it only after the flag cleared (input batching / binding echo),
    /// which would read as a user edit and freeze the derive forever.</summary>
    private string _lastDerived = string.Empty;

    public CloneFromUrlComponent()
    {
        InitializeComponent();
    }

    public CloneFromUrlComponent(CloneFromUrlViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
    }

    /// <summary>
    /// Keeps the folder name in step with the URL — the last URL segment, the name
    /// <c>git clone</c> itself would pick — until the user edits the field by hand.
    /// </summary>
    private void OnUrlBoxTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (_nameEdited) return;
        if (sender is not TextBox urlBox || this.FindControl<TextBox>("NameBox") is not { } nameBox) return;

        var derived = GitUrlParser.DeriveRepoName(urlBox.Text) ?? string.Empty;
        if (nameBox.Text != derived)
        {
            nameBox.Text = derived;
        }

        _lastDerived = derived;
    }

    private void OnNameBoxTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (_nameEdited || sender is not TextBox nameBox) return;
        if (nameBox.Text == _lastDerived) return;
        _nameEdited = true;
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
