using System.ComponentModel;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Threading;
using SukiUI.Controls;
using Tools.Helpers;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.ViewModels.Pages;
using Tools.ViewModels.Windows;
using Tools.Views.Components;
using Tools.Views.Pages;

namespace Tools.Views.Windows;

/// <summary>
/// Main application window: the Repositories page is the permanent content, and a
/// floating tool drawer overlays it with the tool components opened from the title-bar
/// tools dropdown. Window chrome (title bar, caption buttons, dragging) is provided by
/// SukiWindow.
/// </summary>
public partial class MainWindow : SukiWindow
{
    private readonly IToolDrawerService _toolDrawer;
    private readonly ToolViewResolver _resolveToolView;
    private readonly IClipboardPasswordService _clipboardPasswordService;
    private readonly WindowMessageHandler _messageHandler;
    private readonly WindowConfigurator _windowConfigurator;

    /// <summary>
    /// Idle window for the header search field before its text is pushed to the Repos
    /// page. The page applies its own debounce on top, so this only coalesces keystrokes
    /// into a single property push per burst.
    /// </summary>
    private const int HeaderSearchDebounceMs = 150;

    /// <summary>
    /// Hover delay before the title-bar tools flyout opens on pointer-over; keeps a
    /// sweep across the title bar from flashing the menu open.
    /// </summary>
    private const int ToolsFlyoutHoverDelayMs = 250;

    /// <summary>
    /// Grace period the pointer may spend outside both the tools button and the open
    /// flyout (e.g. crossing the gap between them) before the flyout closes.
    /// </summary>
    private const int ToolsFlyoutCloseDelayMs = 350;

    /// <summary>Debounces the header search pushes (see <see cref="HeaderSearchDebounceMs"/>).</summary>
    private readonly UiDebounce _searchDebounce = new(HeaderSearchDebounceMs);

    /// <summary>
    /// Open timer: fires while the pointer rests on the tools button. Close timer:
    /// fires after the pointer has left both the button and the flyout. The flyout's
    /// own move-away dismiss (TransientWithDismissOnPointerMoveAway) is NOT used — it
    /// closes mid-travel while the pointer crosses from the button into the menu.
    /// </summary>
    private DispatcherTimer? _toolsFlyoutOpenTimer;
    private DispatcherTimer? _toolsFlyoutCloseTimer;

    /// <summary>
    /// True while the code-behind is mirroring state INTO the search field (from the
    /// Repos filter); the TextChanged handler must not echo those writes back into the
    /// page.
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
    private ContentControl ToolDrawerHost = null!;
    private Border ToolDrawerResizer = null!;
    private ItemsControl ToastHost = null!;
    private Button ToolsButton = null!;

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
        ContentArea = this.FindControl<ContentControl>("ContentArea")!;
        ToolDrawerHost = this.FindControl<ContentControl>("ToolDrawerHost")!;
        ToolDrawerResizer = this.FindControl<Border>("ToolDrawerResizer")!;
        ToastHost = this.FindControl<ItemsControl>("ToastHost")!;
        ToolsButton = this.FindControl<Button>("ToolsButton")!;
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
        IToolDrawerService toolDrawer,
        ToolViewResolver resolveToolView,
        ReposPage reposPage,
        IClipboardPasswordService clipboardPasswordService,
        INotificationService notificationService)
    {
        _toolDrawer = toolDrawer;
        _resolveToolView = resolveToolView;
        _clipboardPasswordService = clipboardPasswordService;

        // Initialize helper classes (Dependency Inversion Principle)
        _messageHandler = new WindowMessageHandler(clipboardPasswordService);
        _windowConfigurator = new WindowConfigurator(this);

        DataContext = viewModel;
        InitializeComponent();
        InitializeWindow();

        // Wire the toast overlay: the service is its DataContext (provides DismissCommand)
        // and its Toasts collection is the items source.
        ToastHost.DataContext = notificationService;
        ToastHost.ItemsSource = notificationService.Toasts;

        // Host the permanent Repositories page content (runs its ViewModel lifecycle).
        AttachRepositoriesPage(reposPage);
    }

    /// <summary>
    /// Hosts the Repositories page as the window's permanent content and starts its
    /// ViewModel lifecycle. The page is a plain constructor dependency: the original
    /// reason for a composition-root hook (ReposPage → DialogService → MainWindow was
    /// a DI cycle) is gone since DialogService reaches the window through
    /// <see cref="IMainWindowProvider"/> at call time. There is no navigation stack.
    /// </summary>
    private void AttachRepositoriesPage(ReposPage reposPage)
    {
        ContentArea.Content = reposPage;

        if (reposPage.DataContext is ReposViewModel viewModel)
        {
            viewModel.PropertyChanged += OnReposViewModelPropertyChanged;
            FireLifecycle(() => viewModel.OnNavigatedToAsync());
        }
    }

    #region Initialization

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
        _toolDrawer.Changed += OnToolDrawerChanged;
        Closed += OnWindowClosed;

        // Drawer resize grip: a horizontal drag on the card's left-edge band widens or
        // narrows the drawer (dragging left widens — the card is docked right).
        PanelResizeController.Attach(
            ToolDrawerResizer,
            this,
            delta => ViewModel.AdjustToolDrawerWidth(delta),
            PanelResizeAxis.Horizontal);

        // Tools dropdown: opens on hover after the delay below. Closing is owned here
        // too: the flyout stays up while the pointer is over the button or the menu,
        // and the close timer fires only after the pointer has left both surfaces.
        // Click-open keeps the native behavior.
        if (ToolsButton.Flyout is MenuFlyout toolsMenu)
        {
            foreach (var item in toolsMenu.Items.OfType<Control>())
            {
                item.PointerEntered += OnToolsFlyoutSurfaceEntered;
                item.PointerExited += OnToolsFlyoutSurfaceExited;
            }
        }
        ToolsButton.PointerEntered += OnToolsButtonPointerEntered;
        ToolsButton.PointerExited += OnToolsButtonPointerExited;
        _toolsFlyoutOpenTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(ToolsFlyoutHoverDelayMs) };
        _toolsFlyoutOpenTimer.Tick += OnToolsFlyoutOpenTimerTick;
        _toolsFlyoutCloseTimer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(ToolsFlyoutCloseDelayMs) };
        _toolsFlyoutCloseTimer.Tick += OnToolsFlyoutCloseTimerTick;
    }

    private void OnWindowClosed(object? sender, EventArgs e)
    {
        _clipboardPasswordService.UnregisterHotKeys();
        _messageHandler.Uninstall(_windowConfigurator.WindowHandle);
        _toolsFlyoutOpenTimer?.Stop();
        _toolsFlyoutCloseTimer?.Stop();

        if (ContentArea.Content is ReposPage { DataContext: ReposViewModel viewModel })
        {
            viewModel.PropertyChanged -= OnReposViewModelPropertyChanged;
        }
        _toolDrawer.Changed -= OnToolDrawerChanged;
        _searchDebounce.Dispose();
        // Background services (SnapIt, NuGet watch) are stopped during application
        // shutdown, not here, so the window does not own their lifecycle.
    }

    /// <summary>
    /// Hover-open for the title-bar tools dropdown: only a pointer that is still
    /// resting on the button when the delay elapses shows the flyout. Entering the
    /// button cancels any pending close, so the menu survives button → menu travel.
    /// </summary>
    private void OnToolsButtonPointerEntered(object? sender, PointerEventArgs e)
    {
        _toolsFlyoutCloseTimer?.Stop();
        if (ToolsButton.Flyout is { IsOpen: false })
        {
            _toolsFlyoutOpenTimer!.Stop();
            _toolsFlyoutOpenTimer.Start();
        }
    }

    private void OnToolsButtonPointerExited(object? sender, PointerEventArgs e)
    {
        _toolsFlyoutOpenTimer?.Stop();
        StartToolsFlyoutCloseTimer();
    }

    private void OnToolsFlyoutSurfaceEntered(object? sender, PointerEventArgs e) => _toolsFlyoutCloseTimer?.Stop();

    private void OnToolsFlyoutSurfaceExited(object? sender, PointerEventArgs e) => StartToolsFlyoutCloseTimer();

    private void StartToolsFlyoutCloseTimer()
    {
        if (ToolsButton.Flyout is { IsOpen: true })
        {
            _toolsFlyoutCloseTimer!.Stop();
            _toolsFlyoutCloseTimer.Start();
        }
    }

    private void OnToolsFlyoutOpenTimerTick(object? sender, EventArgs e)
    {
        _toolsFlyoutOpenTimer?.Stop();
        if (ToolsButton.IsPointerOver && ToolsButton.Flyout is { IsOpen: false } flyout)
        {
            flyout.ShowAt(ToolsButton);
        }
    }

    private void OnToolsFlyoutCloseTimerTick(object? sender, EventArgs e)
    {
        _toolsFlyoutCloseTimer?.Stop();
        if (ToolsButton.Flyout is { IsOpen: true } flyout)
        {
            flyout.Hide();
        }
    }

    /// <summary>
    /// Mirrors Repos filter changes INTO the header search field so page-side actions
    /// (the Clear chip) are reflected while the user is not typing in the field.
    /// </summary>
    private void OnReposViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName != nameof(ReposViewModel.FilterText)
            || HeaderSearchBox is null
            || HeaderSearchBox.IsFocused
            || _syncingSearchText)
        {
            return;
        }

        var text = sender is ReposViewModel viewModel ? viewModel.FilterText ?? string.Empty : string.Empty;
        if (HeaderSearchBox.Text == text)
        {
            return;
        }

        _syncingSearchText = true;
        HeaderSearchBox.Text = text;
        _syncingSearchText = false;
        UpdateHeaderSearchChrome();
    }

    #endregion

    #region Tool drawer

    /// <summary>
    /// Clicks on the drawer's transparent backdrop (anywhere over the page outside
    /// the drawer card) close the drawer.
    /// </summary>
    private void OnToolDrawerBackdropPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        _toolDrawer.Close();
        e.Handled = true;
    }

    /// <summary>
    /// Mirrors the drawer service state into the visuals: swaps the hosted component
    /// per tool selection, clearing it on close. Components are resolved fresh from DI
    /// per selection (like the former transient pages), and the hosted ViewModel's
    /// lifecycle hooks run on open/close so transient subscriptions to singleton
    /// services (NuGet watch, SnapIt state) are attached and detached symmetrically.
    /// Drawer-hosted dialogs receive their open payload (request state + completion
    /// source) through <see cref="IToolDrawerContextReceiver"/> right after hosting.
    /// </summary>
    private void OnToolDrawerChanged()
    {
        // Teardown the current component first: its ViewModel detaches from singleton
        // services in OnNavigatedFromAsync.
        if (ToolDrawerHost.Content is Control previous
            && previous.DataContext is PageViewModelBase outgoingVm)
        {
            FireLifecycle(() => outgoingVm.OnNavigatedFromAsync());
        }

        if (!_toolDrawer.IsOpen
            || ToolComponentMapper.Find(_toolDrawer.SelectedToolKey) is not { } tool
            || _resolveToolView(tool.Key) is not Control view)
        {
            ToolDrawerHost.Content = null;
            return;
        }

        ToolDrawerHost.Content = view;

        // Deliver the open's context to the hosted component. Tools pass none — the
        // delivery still happens (as a null payload) so a receiver can seed itself (the
        // OpenCode settings drawer seeds from settings + the bar's selected repo);
        // payload-typed receivers treat such deliveries through their null tolerance.
        _ = DeliverDrawerContextAsync(view.DataContext, _toolDrawer.Context);

        if (view.DataContext is PageViewModelBase incomingVm)
        {
            FireLifecycle(() => incomingVm.OnNavigatedToAsync());
        }
    }

    /// <summary>
    /// Delivers the drawer open's context to the hosted component's ViewModel, matched
    /// against the receiver's declared payload type. A context of another kind — the
    /// tools-dropdown opens carry none — delivers as <c>null</c>, so the receiver's own
    /// null tolerance decides: the OpenCode drawer seeds anyway, the dialog receivers
    /// no-op like their former type-guards did. Failures are logged rather than thrown
    /// (fire-and-forget mirrors <see cref="FireLifecycle"/>); the synchronous prefix
    /// still runs inline, keeping the per-open seeding ahead of the lifecycle hooks.
    /// </summary>
    private static async Task DeliverDrawerContextAsync(object? dataContext, object? context)
    {
        if (dataContext is not IToolDrawerContextReceiver receiver)
        {
            return;
        }

        try
        {
            switch (receiver)
            {
                case IToolDrawerContextReceiver<AddRepositoriesDrawerContext> addRepositories:
                    await addRepositories.OnDrawerContextAsync(context as AddRepositoriesDrawerContext);
                    break;
                case IToolDrawerContextReceiver<ReposSettingsDrawerContext> reposSettings:
                    await reposSettings.OnDrawerContextAsync(context as ReposSettingsDrawerContext);
                    break;
                case IToolDrawerContextReceiver<CommitHistoryContext> commitHistory:
                    await commitHistory.OnDrawerContextAsync(context as CommitHistoryContext);
                    break;
                case IToolDrawerContextReceiver<OpenCodeSettingsContext> openCodeSettings:
                    await openCodeSettings.OnDrawerContextAsync(context as OpenCodeSettingsContext);
                    break;
            }
        }
        catch (Exception ex)
        {
            Serilog.Log.Logger.Error(ex, "Tool drawer context delivery threw");
        }
    }

    /// <summary>
    /// Invokes an asynchronous ViewModel lifecycle hook, surfacing failures via the
    /// logger instead of silently swallowing them. Fire-and-forget mirrors the former
    /// navigation service's handling of the same hooks.
    /// </summary>
    private static void FireLifecycle(Func<Task> hook)
    {
        _ = Task.Run(async () =>
        {
            try
            {
                await hook();
            }
            catch (Exception ex)
            {
                Serilog.Log.Logger.Error(ex, "ViewModel lifecycle hook threw");
            }
        });
    }

    #endregion

    #region Header search

    /// <summary>
    /// Ctrl+F focuses the header search field from anywhere in the app. Handled on the
    /// window so it works regardless of which control holds keyboard focus (a TextBox
    /// lets the unhandled gesture bubble). Escape closes the tool drawer.
    /// </summary>
    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (!e.Handled && e.Key == Key.Escape && _toolDrawer.IsOpen)
        {
            _toolDrawer.Close();
            e.Handled = true;
            return;
        }
        if (e.Key == Key.F && e.KeyModifiers.HasFlag(KeyModifiers.Control) && HeaderSearchBox is not null)
        {
            HeaderSearchBox.Focus();
            HeaderSearchBox.SelectAll();
            e.Handled = true;
            return;
        }
        base.OnKeyDown(e);
    }

    /// <summary>
    /// Header search typed text: debounce-push the term into the Repos page's filter
    /// (an empty field clears it).
    /// </summary>
    private void OnHeaderSearchTextChanged(object? sender, TextChangedEventArgs e)
    {
        UpdateHeaderSearchChrome();
        if (_syncingSearchText) return;

        _searchDebounce.Debounce(() => ApplyHeaderSearch(HeaderSearchBox.Text ?? string.Empty));
    }

    /// <summary>Enter applies the term immediately; Escape clears it.</summary>
    private void OnHeaderSearchKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            _searchDebounce.Cancel();
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
    /// Writes the term into the Repos page's filter. The page ViewModel lives for the
    /// window lifetime, so the push is safe whether the page is freshly initialized or
    /// already showing (its own debounce re-filters).
    /// </summary>
    private void ApplyHeaderSearch(string text)
    {
        if (ContentArea.Content is ReposPage { DataContext: ReposViewModel viewModel })
        {
            viewModel.FilterText = text;
        }
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
}
