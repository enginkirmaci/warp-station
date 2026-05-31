using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Providers;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the main window of the application.
/// </summary>
public partial class MainWindowViewModel : ViewModelBase
{
    /// <summary>
    /// Gets the title of the application.
    /// </summary>
    public string ApplicationTitle { get; } = "Dev Tools";

    /// <summary>
    /// Gets the collection of menu items for navigation.
    /// </summary>
    public IReadOnlyCollection<NavigationItem> MenuItems { get; }

    /// <summary>
    /// Gets or sets the currently displayed view.
    /// </summary>
    private Control? _currentView;
    public Control? CurrentView
    {
        get => _currentView;
        set => SetProperty(ref _currentView, value);
    }

    public MainWindowViewModel()
    {
        MenuItems = NavigationProvider.GetNavigationMenuItems();
    }
}