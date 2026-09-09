using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.Views.Components.Repo;

/// <summary>
/// Repo detail header at the top of the Repositories page (back link, repo name + star,
/// branch / GitHub URL line, tag chip row, tab row, Open in GitHub + kebab). Its
/// DataContext is the singleton <see cref="BottomBarViewModel"/> attached by ReposPage —
/// the same VM as the bottom bar, so the tabs here drive the bar's panel.
/// <para>
/// The add-tag flyout is wired here (Enter / Add button share one path): read the box,
/// let the shell VM add the tag (its duplicate check toasts a warning), clear the box
/// only when the tag landed so a typo survives to be fixed, then refocus for the next
/// entry; Escape hides the flyout.
/// </para>
/// </summary>
public partial class RepoHeader : UserControl
{
    public RepoHeader()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>Adds the input box's text as a tag on the selected repo (Enter and the
    /// Add button both land here). Keeps the flyout open for back-to-back entries.</summary>
    private async void AddTagFromInput()
    {
        if (DataContext is not BottomBarViewModel vm) return;
        if (this.FindControl<TextBox>("NewTagInput") is not { } input) return;

        var added = await vm.AddRepoTagAsync(input.Text);
        if (added)
        {
            input.Text = string.Empty;
        }

        input.Focus();
    }

    private void NewTagInput_OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            e.Handled = true;
            AddTagFromInput();
        }
        else if (e.Key == Key.Escape)
        {
            e.Handled = true;
            (this.FindControl<Button>("AddTagButton")?.Flyout as Flyout)?.Hide();
        }
    }

    private void AddTagConfirm_OnClick(object? sender, RoutedEventArgs e)
    {
        AddTagFromInput();
    }

    /// <summary>Each open starts from a clean, focused input (a fresh repo must not
    /// inherit the previous one's half-typed name).</summary>
    private void AddTagFlyout_OnOpened(object? sender, System.EventArgs e)
    {
        if (this.FindControl<TextBox>("NewTagInput") is { } input)
        {
            input.Text = string.Empty;
            input.Focus();
        }
    }
}
