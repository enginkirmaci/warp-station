using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Styling;
using Tools.Library.Entities;
using Tools.ViewModels.Components;
using Tools.ViewModels.Pages;
using Tools.Views.Components;

namespace Tools.Views.Pages;

public partial class ReposPage : UserControl
{
    public ReposViewModel ViewModel { get; }

    /// <summary>The bottom bar's singleton ViewModel; row presses hand their repo to it.</summary>
    public BottomBarViewModel BottomBarViewModel { get; }

    private ListBox? _reposList;

    public ReposPage()
    {
        InitializeComponent();
    }

    public ReposPage(ReposViewModel viewModel, BottomBarViewModel bottomBarViewModel)
    {
        ViewModel = viewModel;
        BottomBarViewModel = bottomBarViewModel;
        DataContext = viewModel;
        InitializeComponent();
        // The bottom bar owns its singleton ViewModel (repo context, repo header, tabs,
        // OpenCode surface); the rest of the page binds to ReposViewModel.
        this.FindControl<BottomBar>("BottomBarControl")!.DataContext = bottomBarViewModel;
        _reposList = this.FindControl<ListBox>("ReposList");
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// A press on a row's card selects that repo for the bottom bar — the first press
    /// reveals the bar, which stays hidden until a repo is picked from the table. The
    /// press is still swallowed before it reaches the ListBoxItem, which would otherwise
    /// select the item and flash the theme's selected-state indicator. Buttons inside
    /// the template sit deeper in the visual tree and handle their own presses first,
    /// so the row chips route their tabs without also re-firing this.
    /// </summary>
    private void OnCardPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        e.Handled = true;
        if (sender is StyledElement { DataContext: Repo repo }
            && e.GetCurrentPoint(null).Properties.IsLeftButtonPressed)
        {
            BottomBarViewModel.OpenForRepo(repo);

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
}
