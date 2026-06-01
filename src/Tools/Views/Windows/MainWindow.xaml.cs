using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Platform;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Windows;
using Tools.Library.Converters;

namespace Tools.Views.Windows;

/// <summary>
/// Main application window with navigation sidebar, content area, and info bar.
/// </summary>
public partial class MainWindow : Window
{
    private readonly INavigationService _navigationService;
    private readonly IClipboardPasswordService _clipboardPasswordService;
    private readonly ISnapItService _snapItService;

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
        ISnapItService snapItService)
    {
        _navigationService = navigationService;
        _clipboardPasswordService = clipboardPasswordService;
        _snapItService = snapItService;
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
        var handle = TryGetPlatformHandle()?.Handle ?? nint.Zero;
        if (handle != nint.Zero)
        {
            _clipboardPasswordService.RegisterHotKeys(handle);
        }
#endif
        Closed += OnWindowClosed;
    }

#if WINDOWS
    private nint TryGetPlatformHandle()
    {
        if (this.TryGetPlatformHandle() is { } platformHandle)
            return platformHandle.Handle;
        return nint.Zero;
    }
#endif

    private void NavigateToPage(string pageKey, int selectedIndex)
    {
        _isNavigatingFromCode = true;
        var pageType = NameToPageTypeConverter.Convert(pageKey);
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
            var pageType = NameToPageTypeConverter.Convert(item.PageKey);
            if (pageType != null)
            {
                _navigationService.Navigate(pageType);
            }
        }
    }

    private void OnNavigated(Type? pageType)
    {
        UpdateBackButtonVisibility();
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
        _navigationService.Navigated -= OnNavigated;
        _navigationService.BackStackChanged -= OnBackStackChanged;
        _snapItService.Stop();
    }

    #endregion

    #region Public Methods

    public void ShowInfoBar(string title, string message, Avalonia.Controls.Notifications.NotificationType severity)
    {
        // InfoBar not available in this Avalonia version - placeholder
    }

    #endregion
}