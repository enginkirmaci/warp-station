using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using SukiUI.Controls;
using Tools.Helpers;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Pages;
using Tools.ViewModels.Windows;
using Tools.Views.Pages;

namespace Tools.Views.Windows;

/// <summary>
/// Main application window with navigation sidebar, content area, and info bar.
/// Window chrome (title bar, caption buttons, dragging) is provided by SukiWindow.
/// </summary>
public partial class MainWindow : SukiWindow
{
    private readonly INavigationService _navigationService;
    private readonly IClipboardPasswordService _clipboardPasswordService;
    private readonly WindowMessageHandler _messageHandler;
    private readonly WindowConfigurator _windowConfigurator;

    private bool _isNavigatingFromCode;

    /// <summary>
    /// Idle window for the header search field before its text is pushed to the Repos
    /// page. The page applies its own debounce on top, so this only coalesces keystrokes
    /// into a single navigation/property push per burst.
    /// </summary>
    private const int HeaderSearchDebounceMs = 150;

    private CancellationTokenSource? _searchDebounce;

    /// <summary>
    /// True while the code-behind is mirroring state INTO the search field (navigation
    /// sync); the TextChanged handler must not echo those writes back into the page.
    /// </summary>
    private bool _syncingSearchText;

    /// <summary>
    /// The header search field lives in the CUSTOM WINDOW TEMPLATE (CustomSukiWindowTheme):
    /// it is a named part of the title bar, so it (and its hint/clear chrome) is resolved
    /// in <see cref="OnApplyTemplate"/> and its event handlers are attached there too — a
    /// ControlTheme has no code-behind to wire them in XAML. Nullable: nothing is set
    /// until the template is applied.
    /// </summary>
    private TextBox? HeaderSearchBox;
    private Border? SearchKbdHint;
    private Button? SearchClearButton;

    // Named XAML elements
    private ContentControl ContentArea = null!;
    private ListBox NavigationListBox = null!;
    private Button BackButton = null!;
    private ItemsControl ToastHost = null!;

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
        ContentArea = this.FindControl<ContentControl>("ContentArea")!;
        NavigationListBox = this.FindControl<ListBox>("NavigationListBox")!;
        BackButton = this.FindControl<Button>("BackButton")!;
        ToastHost = this.FindControl<ItemsControl>("ToastHost")!;
    }

    /// <inheritdoc/>
    protected override void OnApplyTemplate(TemplateAppliedEventArgs e)
    {
        base.OnApplyTemplate(e);

        HeaderSearchBox = e.NameScope.Find<TextBox>("HeaderSearchBox");
        SearchKbdHint = e.NameScope.Find<Border>("SearchKbdHint");
        SearchClearButton = e.NameScope.Find<Button>("SearchClearButton");

        if (HeaderSearchBox is not null)
        {
            HeaderSearchBox.TextChanged += OnHeaderSearchTextChanged;
            HeaderSearchBox.KeyDown += OnHeaderSearchKeyDown;
        }
        if (SearchClearButton is not null)
        {
            SearchClearButton.Click += OnSearchClearClick;
        }
        UpdateHeaderSearchChrome();
    }

    /// <summary>
    /// Gets the ViewModel for this window.
    /// </summary>
    public MainWindowViewModel ViewModel => (MainWindowViewModel)DataContext!;

    public MainWindow(
        MainWindowViewModel viewModel,
        INavigationService navigationService,
        IClipboardPasswordService clipboardPasswordService,
        INotificationService notificationService)
    {
        _navigationService = navigationService;
        _clipboardPasswordService = clipboardPasswordService;

        // Initialize helper classes (Dependency Inversion Principle)
        _messageHandler = new WindowMessageHandler(clipboardPasswordService);
        _windowConfigurator = new WindowConfigurator(this);

        DataContext = viewModel;
        InitializeComponent();
        InitializeNavigation();
        InitializeWindow();

        // Wire the toast overlay: the service is its DataContext (provides DismissCommand)
        // and its Toasts collection is the items source.
        ToastHost.DataContext = notificationService;
        ToastHost.ItemsSource = notificationService.Toasts;
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
#else
        // Non-Windows: the hotkey listener owns its platform connection (X11 display)
        // and needs no window handle; the message-handler hook is Windows-only.
        _clipboardPasswordService.RegisterHotKeys(nint.Zero);
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
        SyncHeaderSearch(pageType);
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
        _searchDebounce?.Cancel();
        _searchDebounce?.Dispose();
        // Background services (SnapIt, NuGet watch) are stopped during application
        // shutdown, not here, so the window does not own their lifecycle.
    }

    #endregion

    #region Header search

    /// <summary>
    /// Ctrl+K focuses the header search field from anywhere in the app. Handled on the
    /// window so it works regardless of which control holds keyboard focus (a TextBox
    /// lets the unhandled gesture bubble).
    /// </summary>
    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (e.Key == Key.K && e.KeyModifiers.HasFlag(KeyModifiers.Control) && HeaderSearchBox is not null)
        {
            HeaderSearchBox.Focus();
            HeaderSearchBox.SelectAll();
            e.Handled = true;
            return;
        }
        base.OnKeyDown(e);
    }

    /// <summary>
    /// Header search typed text: debounce-push the term to the Repos page, navigating
    /// there first when the user is on another page. An empty field clears the Repos
    /// filter when the page is open; leaving a non-Repos page just clears the field.
    /// </summary>
    private void OnHeaderSearchTextChanged(object? sender, TextChangedEventArgs e)
    {
        UpdateHeaderSearchChrome();
        if (_syncingSearchText) return;

        var text = HeaderSearchBox.Text ?? string.Empty;
        if (text.Length == 0 && ContentArea.Content is not ReposPage) return;

        _searchDebounce?.Cancel();
        _searchDebounce?.Dispose();
        _searchDebounce = new CancellationTokenSource();
        var token = _searchDebounce.Token;
        _ = Task.Run(async () =>
        {
            try
            {
                await Task.Delay(HeaderSearchDebounceMs, token);
                Dispatcher.UIThread.Post(() =>
                {
                    if (token.IsCancellationRequested) return;
                    ApplyHeaderSearch(HeaderSearchBox.Text ?? string.Empty);
                });
            }
            catch (OperationCanceledException) { }
        });
    }

    /// <summary>Enter applies the term immediately; Escape clears it.</summary>
    private void OnHeaderSearchKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            _searchDebounce?.Cancel();
            ApplyHeaderSearch(HeaderSearchBox.Text ?? string.Empty);
            e.Handled = true;
        }
        else if (e.Key == Key.Escape)
        {
            HeaderSearchBox.Text = string.Empty;
            e.Handled = true;
        }
    }

    private void OnSearchClearClick(object? sender, RoutedEventArgs e)
    {
        if (HeaderSearchBox is not null) HeaderSearchBox.Text = string.Empty;
    }

    /// <summary>
    /// Navigates to the Repos page when needed and writes the term into the page's
    /// filter. The page ViewModel is transient (rebuilt per navigation), so the push is
    /// safe whether the page was just created (initialization picks the term up) or is
    /// already showing (its own debounce re-filters).
    /// </summary>
    private void ApplyHeaderSearch(string text)
    {
        if (ContentArea.Content is not ReposPage reposPage)
        {
            if (text.Length == 0) return;
            _navigationService.Navigate(typeof(ReposPage));
            reposPage = ContentArea.Content as ReposPage;
            if (reposPage is null) return;
        }

        if (reposPage.DataContext is ReposViewModel viewModel)
        {
            viewModel.FilterText = text;
        }
    }

    /// <summary>
    /// Mirrors state INTO the search field on navigation: on Repos it shows that page's
    /// filter, elsewhere it clears. Skipped while the field holds focus — a
    /// search-triggered navigation lands here mid-typing, and the user's text (not the
    /// fresh page's empty filter) is the authority while they type.
    /// </summary>
    private void SyncHeaderSearch(Type? pageType)
    {
        if (HeaderSearchBox is null || HeaderSearchBox.IsFocused) return;

        var text = pageType == typeof(ReposPage)
            && ContentArea.Content is ReposPage { DataContext: ReposViewModel viewModel }
                ? viewModel.FilterText ?? string.Empty
                : string.Empty;

        if (HeaderSearchBox.Text == text) return;
        _syncingSearchText = true;
        HeaderSearchBox.Text = text;
        _syncingSearchText = false;
        UpdateHeaderSearchChrome();
    }

    /// <summary>The shortcut hint yields to the clear button once there is text.</summary>
    private void UpdateHeaderSearchChrome()
    {
        if (HeaderSearchBox is null || SearchKbdHint is null || SearchClearButton is null) return;

        var hasText = !string.IsNullOrEmpty(HeaderSearchBox.Text);
        SearchKbdHint.IsVisible = !hasText;
        SearchClearButton.IsVisible = hasText;
    }

    #endregion

    #region Public Methods

    #endregion
}