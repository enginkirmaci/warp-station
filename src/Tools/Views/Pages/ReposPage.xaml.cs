using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Avalonia.Styling;
using Tools.Library.Entities;
using Tools.ViewModels.Components;
using Tools.ViewModels.Components.BottomBar;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class ReposPage : UserControl
{
    public ReposViewModel ViewModel { get; }

    /// <summary>The bottom bar's singleton ViewModel; row presses hand their repo to it.</summary>
    public BottomBarViewModel BottomBarViewModel { get; }

    private ListBox? _reposList;

    /// <summary>
    /// XAML infrastructure requires a public parameterless ctor on the x:Class type;
    /// runtime construction must go through the DI one — both ViewModels have to be
    /// set BEFORE InitializeComponent (the bottom bar's XAML binds
    /// #Root.BottomBarViewModel). Never call this.
    /// </summary>
    public ReposPage()
    {
        throw new InvalidOperationException(
            "ReposPage requires (ReposViewModel, BottomBarViewModel) — resolve it through DI.");
    }

    public ReposPage(ReposViewModel viewModel, BottomBarViewModel bottomBarViewModel)
    {
        // Both ViewModels are assigned before InitializeComponent: the bottom bar's
        // DataContext binds to BottomBarViewModel in XAML ({Binding
        // #Root.BottomBarViewModel}), so it must already be set when the bar's
        // bindings initialize during load. The bar never inherits the page
        // DataContext, so the assignment below cannot cast-fail the bar's compiled
        // bindings regardless of order.
        ViewModel = viewModel;
        BottomBarViewModel = bottomBarViewModel;
        InitializeComponent();
        _reposList = this.FindControl<ListBox>("ReposList");
        // The bar owns its singleton ViewModel (repo context, repo header, tabs);
        // the rest of the page binds to ReposViewModel. The OpenCode settings
        // drawer seeds from the same singleton when it opens.
        DataContext = viewModel;
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// A press on a row's card routes that repo to the bottom bar — the first press
    /// reveals the bar (which stays hidden until a repo is picked from the table) and
    /// a press on the already-highlighted row closes the bar entirely and clears the
    /// row's highlight (see <see cref="BottomBarViewModel.ToggleForRepo"/>). The
    /// press is still swallowed before it reaches the ListBoxItem, which would otherwise
    /// select the item and flash the theme's selected-state indicator. Buttons inside
    /// the template sit deeper in the visual tree and handle their own presses first,
    /// so the row chips route their tabs without also re-firing this.
    /// </summary>
    private void OnCardPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is StyledElement { DataContext: Repo repo }
            && e.GetCurrentPoint(null).Properties.IsLeftButtonPressed)
        {
            e.Handled = true;
            BottomBarViewModel.ToggleForRepo(repo);

            // Opening the panel shrinks the table's viewport, which can leave the row
            // just clicked hidden under it; once the layout has settled, scroll it back
            // into view.
            Avalonia.Threading.Dispatcher.UIThread.Post(
                () => _reposList?.ScrollIntoView(repo),
                Avalonia.Threading.DispatcherPriority.ApplicationIdle);
        }
    }

    /// <summary>
    /// The repo cards are non-interactive containers — selection now lives in the bottom
    /// bar (see <see cref="OnCardPointerPressed"/>) — so clear the ListBox selection
    /// immediately to suppress the theme's selected-state indicator (the pill shown on a
    /// clicked card).
    /// </summary>
    private void OnRepoSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ListBox { SelectedItem: not null } listBox)
        {
            listBox.SelectedItem = null;
        }
    }

    /// <summary>
    /// Scrolls a repo's row into view once layout settles — the global search's repo
    /// activation (same posted pattern as the row press above: selecting a repo reveals
    /// the bar, which shrinks the table's viewport).
    /// </summary>
    public void RevealRepo(Repo repo)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(
            () => _reposList?.ScrollIntoView(repo),
            Avalonia.Threading.DispatcherPriority.ApplicationIdle);
    }
}
