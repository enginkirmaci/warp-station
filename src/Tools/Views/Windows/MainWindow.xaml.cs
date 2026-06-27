using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.Helpers;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Windows;

namespace Tools.Views.Windows;

/// <summary>
/// Main application window with navigation sidebar, content area, and info bar.
/// </summary>
public partial class MainWindow : Window
{
    private readonly INavigationService _navigationService;
    private readonly IClipboardPasswordService _clipboardPasswordService;
    private readonly ISnapItService _snapItService;
    private readonly INugetLocalService _nugetLocalService;
    private readonly WindowMessageHandler _messageHandler;
    private readonly WindowConfigurator _windowConfigurator;

    private bool _isNavigatingFromCode;

    // Named XAML elements
    private ContentControl ContentArea = null!;
    private ListBox NavigationListBox = null!;
    private Button BackButton = null!;

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
        ContentArea = this.FindControl<ContentControl>("ContentArea")!;
        NavigationListBox = this.FindControl<ListBox>("NavigationListBox")!;
        BackButton = this.FindControl<Button>("BackButton")!;
    }

    /// <summary>
    /// Gets the ViewModel for this window.
    /// </summary>
    public MainWindowViewModel ViewModel => (MainWindowViewModel)DataContext!;

    public MainWindow(
        MainWindowViewModel viewModel,
        INavigationService navigationService,
        IClipboardPasswordService clipboardPasswordService,
        ISnapItService snapItService,
        INugetLocalService nugetLocalService)
    {
        _navigationService = navigationService;
        _clipboardPasswordService = clipboardPasswordService;
        _snapItService = snapItService;
        _nugetLocalService = nugetLocalService;

        // Initialize helper classes (Dependency Inversion Principle)
        _messageHandler = new WindowMessageHandler(clipboardPasswordService);
        _windowConfigurator = new WindowConfigurator(this);

        DataContext = viewModel;
        InitializeComponent();
        InitializeNavigation();
        InitializeWindow();
    }

    #region Initialization

    private void InitializeNavigation()
    {
        // Wire up the navigation service's ContentControl
        _navigationService.SetContentControl(ContentArea);
        _navigationService.Navigated += OnNavigated;
        _navigationService.BackStackChanged += OnBackStackChanged;
        UpdateBackButtonVisibility();
        // Navigate to dashboard
        NavigateToPage("DashboardPage", 0);
    }

    private void InitializeWindow()
    {
#if WINDOWS
        _windowConfigurator.Configure();

        var handle = ((Avalonia.Controls.TopLevel)this).TryGetPlatformHandle()?.Handle ?? nint.Zero;
        if (handle != nint.Zero)
        {
            _clipboardPasswordService.RegisterHotKeys(handle);
            _messageHandler.Install(handle);
        }
#endif
        Closed += OnWindowClosed;
    }

    private void NavigateToPage(string pageKey, int selectedIndex)
    {
        _isNavigatingFromCode = true;
        var pageType = PageNavigationMapper.Convert(pageKey);
        if (pageType != null)
        {
            _navigationService.Navigate(pageType);
        }
        NavigationListBox.SelectedIndex = selectedIndex;
        _isNavigatingFromCode = false;
    }

    #endregion

    #region Event Handlers

    private void BackButton_Click(object? sender, Avalonia.Interactivity.RoutedEventArgs e)
    {
        if (_navigationService.CanGoBack)
        {
            _navigationService.GoBack();
        }
    }

    private void TitleBar_PointerPressed(object? sender, PointerPressedEventArgs e)
    {
        var point = e.GetCurrentPoint(this);
        if (point.Properties.IsLeftButtonPressed)
        {
            BeginMoveDrag(e);
        }
    }

    private void NavigationListBox_SelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (_isNavigatingFromCode) return;
        if (e.AddedItems.Count > 0 && e.AddedItems[0] is Tools.Library.Entities.NavigationItem item)
        {
            // Skip separators and headers
            if (item.PageKey == "__separator__" || item.PageKey == "__header__")
            {
                _isNavigatingFromCode = true;
                // Re-select the previous valid item
                if (e.RemovedItems.Count > 0)
                {
                    NavigationListBox.SelectedItem = e.RemovedItems[0];
                }
                _isNavigatingFromCode = false;
                return;
            }
            var pageType = PageNavigationMapper.Convert(item.PageKey);
            if (pageType != null)
            {
                _navigationService.Navigate(pageType);
            }
        }
    }

    private void OnNavigated(Type? pageType)
    {
        UpdateBackButtonVisibility();
        SyncSidebarSelection(pageType);
    }

    private void SyncSidebarSelection(Type? pageType)
    {
        if (pageType == null || ViewModel.MenuItems == null)
        {
            return;
        }

        var pageName = pageType.Name;
        var match = ViewModel.MenuItems.FirstOrDefault(item =>
            !string.IsNullOrEmpty(item.PageKey) &&
            item.PageKey != "__separator__" &&
            item.PageKey != "__header__" &&
            string.Equals(item.PageKey, pageName, StringComparison.OrdinalIgnoreCase));

        if (match == null || ReferenceEquals(NavigationListBox.SelectedItem, match))
        {
            return;
        }

        _isNavigatingFromCode = true;
        NavigationListBox.SelectedItem = match;
        _isNavigatingFromCode = false;
    }

    private void OnBackStackChanged()
    {
        UpdateBackButtonVisibility();
    }

    private void UpdateBackButtonVisibility()
    {
        BackButton.IsVisible = _navigationService.CanGoBack;
    }

    private void OnWindowClosed(object? sender, EventArgs e)
    {
        _clipboardPasswordService.UnregisterHotKeys();
        _messageHandler.Uninstall(_windowConfigurator.WindowHandle);

        _navigationService.Navigated -= OnNavigated;
        _navigationService.BackStackChanged -= OnBackStackChanged;
        _snapItService.Stop();
        _nugetLocalService.Stop();
    }

    #endregion

    #region Public Methods

    #endregion
}