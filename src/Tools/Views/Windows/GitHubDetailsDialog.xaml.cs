using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Markup.Xaml;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Windows;

namespace Tools.Views.Windows;

/// <summary>
/// Modal dialog listing one repo's open pull requests and issues (pull requests first,
/// no tabs). A thin view: fetching, list state, and the link launches live in
/// <see cref="GitHubDetailsViewModel"/>, which kicks an initial fetch on construction
/// and re-runs it via the header's Refresh button.
/// </summary>
public partial class GitHubDetailsDialog : Window
{
    /// <summary>
    /// Gets the ViewModel backing this dialog.
    /// </summary>
    public GitHubDetailsViewModel ViewModel { get; }

    /// <summary>
    /// Initializes a new instance of the <see cref="GitHubDetailsDialog"/> class.
    /// </summary>
    /// <param name="repo">The repo whose GitHub activity is shown.</param>
    /// <param name="gitHubService">The GitHub query service (fetch + cache).</param>
    /// <param name="processLauncher">Opens item/repo links in the browser.</param>
    public GitHubDetailsDialog(Repo repo, IGitHubService gitHubService, IProcessLauncher processLauncher)
    {
        ViewModel = new GitHubDetailsViewModel(repo, gitHubService, processLauncher);
        DataContext = ViewModel;
        InitializeComponent();
    }

    /// <summary>
    /// Designer/XAML-compiler constructor. Not used at runtime — the app always
    /// constructs the dialog with its dependencies.
    /// </summary>
    public GitHubDetailsDialog()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// Starts a window move when the user presses on the header's drag surface (the
    /// transparent Border behind the header content). The dialog is borderless on
    /// Wayland — there is no OS titlebar to grab — so this is the drag handle. The
    /// refresh / close / repo-link buttons sit on top of the surface and handle their
    /// own presses first, so their clicks never start a move.
    /// </summary>
    private void OnHeaderPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (e.GetCurrentPoint(this).Properties.IsLeftButtonPressed)
        {
            BeginMoveDrag(e);
        }
    }
}
