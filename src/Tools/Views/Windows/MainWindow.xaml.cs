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
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.ViewModels.Pages;
using Tools.ViewModels.Windows;
using Tools.Views.Components;
using Tools.Views.Components.BottomBar;
using Tools.Views.Pages;
using Avalonia.VisualTree;

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
    /// Idle window for the header search field before its term reaches the global
    /// search ViewModel. The ViewModel's own generation counter coalesces results, so
    /// this only collapses keystroke bursts into one search per pause.
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

    /// <summary>Debounces the header search terms (see <see cref="HeaderSearchDebounceMs"/>).</summary>
    private readonly UiDebounce _searchDebounce = new(HeaderSearchDebounceMs);

    /// <summary>The global search dropdown's ViewModel (window-VM property, injected
    /// here directly so the activation actions reach it without a cast).</summary>
    private readonly GlobalSearchViewModel _globalSearch;

    /// <summary>
    /// Open timer: fires while the pointer rests on the tools button. Close timer:
    /// fires after the pointer has left both the button and the flyout. The flyout's
    /// own move-away dismiss (TransientWithDismissOnPointerMoveAway) is NOT used — it
    /// closes mid-travel while the pointer crosses from the button into the menu.
    /// </summary>
    private DispatcherTimer? _toolsFlyoutOpenTimer;
    private DispatcherTimer? _toolsFlyoutCloseTimer;

    /// <summary>
    /// The header search field, its dropdown and its hint/clear chrome live in the
    /// CUSTOM WINDOW TEMPLATE (CustomSukiWindowTheme): they are named parts of the
    /// title bar, resolved in <see cref="OnApplyTemplate"/> where their event handlers
    /// are attached too — a ControlTheme has no code-behind to wire them in XAML.
    /// Nullable: nothing is set until the template is applied.
    /// </summary>
    private TextBox? HeaderSearchBox;
    private Popup? HeaderSearchPopup;
    private ListBox? HeaderSearchResultsList;
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

        // Unsubscribe from the previous template parts first: a re-apply (theme
        // switch, template invalidation) hands back NEW control instances and
        // re-subscribing without dropping the old handlers would double-fire the
        // search pushes.
        if (HeaderSearchBox is not null)
        {
            HeaderSearchBox.TextChanged -= OnHeaderSearchTextChanged;
            HeaderSearchBox.KeyDown -= OnHeaderSearchKeyDown;
        }
        if (SearchClearButton is not null)
        {
            SearchClearButton.Click -= OnSearchClearClick;
        }
        if (HeaderSearchResultsList is not null)
        {
            HeaderSearchResultsList.Tapped -= OnSearchResultTapped;
        }

        HeaderSearchBox = e.NameScope.Find<TextBox>("HeaderSearchBox");
        HeaderSearchPopup = e.NameScope.Find<Popup>("HeaderSearchPopup");
        HeaderSearchResultsList = e.NameScope.Find<ListBox>("HeaderSearchResultsList");
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
        if (HeaderSearchResultsList is not null)
        {
            HeaderSearchResultsList.Tapped += OnSearchResultTapped;
        }
        if (HeaderSearchPopup is not null)
        {
            HeaderSearchPopup.IsOpen = false;
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
        SettingsPage settingsPage,
        NotesPage notesPage,
        IClipboardPasswordService clipboardPasswordService,
        INotificationService notificationService)
    {
        _toolDrawer = toolDrawer;
        _resolveToolView = resolveToolView;
        _clipboardPasswordService = clipboardPasswordService;
        _globalSearch = viewModel.GlobalSearch;
        _reposPage = reposPage;
        _settingsPage = settingsPage;
        _notesPage = notesPage;

        // Initialize helper classes (Dependency Inversion Principle)
        _messageHandler = new WindowMessageHandler(clipboardPasswordService);
        _windowConfigurator = new WindowConfigurator(this);

        DataContext = viewModel;
        InitializeComponent();
        InitializeWindow();

        // The title-bar gear toggles the Settings page: it swaps in when another page
        // is showing and swaps back to Repositories when Settings is already open
        // (the page's back link does the same).
        viewModel.SettingsRequested += OnSettingsRequested;
        settingsPage.BackRequested += OnBackToRepositoriesRequested;

        // The title-bar note button follows the same toggle pattern for the Notes page.
        viewModel.NotesRequested += OnNotesRequested;
        notesPage.BackRequested += OnBackToRepositoriesRequested;

        // The global search dropdown opens/closes on its ViewModel's result pushes;
        // the activation actions (page swaps) stay here in the window.
        _globalSearch.PropertyChanged += OnGlobalSearchPropertyChanged;

        // Wire the toast overlay: the service is its DataContext (provides DismissCommand)
        // and its Toasts collection is the items source.
        ToastHost.DataContext = notificationService;
        ToastHost.ItemsSource = notificationService.Toasts;

        // Host the permanent Repositories page content (runs its ViewModel lifecycle).
        AttachRepositoriesPage(reposPage);
    }

    private readonly ReposPage _reposPage;
    private readonly SettingsPage _settingsPage;
    private readonly NotesPage _notesPage;

    /// <summary>Toggles the dedicated Settings page: swaps it into the content area
    /// (no navigation stack — the Repositories page instance stays alive and is
    /// swapped back whole), or — while Settings is already showing — swaps back to
    /// Repositories, so the title-bar gear doubles as a close. The page reloads every
    /// edited section on each show.</summary>
    private void OnSettingsRequested(object? sender, EventArgs e)
    {
        if (ContentArea.Content is SettingsPage)
        {
            OnBackToRepositoriesRequested(sender, e);
            return;
        }

        ContentArea.Content = _settingsPage;
        if (_settingsPage.DataContext is SettingsPageViewModel viewModel)
        {
            FireLifecycle(viewModel.OnNavigatedToAsync);
        }
    }

    /// <summary>The Settings page's back link: the Repositories page instance returns as-is.</summary>
    private void OnBackToRepositoriesRequested(object? sender, EventArgs e)
    {
        if (ContentArea.Content is not ReposPage)
        {
            ContentArea.Content = _reposPage;
        }
    }

    /// <summary>Toggles the dedicated Notes page (the title-bar note button doubles as a
    /// close, like the gear). The page reloads on every show.</summary>
    private void OnNotesRequested(object? sender, EventArgs e)
    {
        if (ContentArea.Content is NotesPage)
        {
            OnBackToRepositoriesRequested(sender, e);
            return;
        }

        ContentArea.Content = _notesPage;
        if (_notesPage.DataContext is NotesPageViewModel viewModel)
        {
            FireLifecycle(viewModel.OnNavigatedToAsync);
        }
    }

    /// <summary>A repo row's note button: always shows the Notes page — opening it when
    /// another page is showing, reloading it when it already is (the row's command has
    /// by then selected the clicked repo in the bottom bar, so the reload auto-expands
    /// that repo's folder in the all-notes tree). No toggle: while repo A's notes are
    /// open, repo B's note button must switch, not close.</summary>
    private void OnRepoNotesRequested(object? sender, EventArgs e)
    {
        if (ContentArea.Content is not NotesPage)
        {
            ContentArea.Content = _notesPage;
        }

        if (_notesPage.DataContext is NotesPageViewModel viewModel)
        {
            FireLifecycle(viewModel.OnNavigatedToAsync);
        }
    }

    /// <summary>
    /// Hosts the Repositories page as the window's permanent content. The ViewModel
    /// lifecycle starts only when the window is actually on screen (<see
    /// cref="OnMainWindowOpened"/>): the first paint must never wait on data work, and
    /// the page's own sequence behind it is repo list first, git status pass behind
    /// that (fire-and-forget, plus the status service's scan-completion re-check). The
    /// page is a plain constructor dependency: the original reason for a
    /// composition-root hook (ReposPage → DialogService → MainWindow was a DI cycle) is
    /// gone since DialogService reaches the window through
    /// <see cref="IMainWindowProvider"/> at call time. There is no navigation stack.
    /// </summary>
    private void AttachRepositoriesPage(ReposPage reposPage)
    {
        ContentArea.Content = reposPage;

        if (reposPage.DataContext is ReposViewModel viewModel)
        {
            // Repo rows' note buttons route here: select the repo in the bottom bar,
            // then show (or reload) the Notes page for it.
            viewModel.NotesRequested += OnRepoNotesRequested;
            Opened += OnMainWindowOpened;
        }
    }

    /// <summary>
    /// One-shot <see cref="Window.Opened"/> handler that starts the app's data lifecycles
    /// on first show (see <see cref="AttachRepositoriesPage"/>): the Repos page's
    /// lifecycle and the bottom bar's session load — the bar's constructor deliberately
    /// does not self-start it, or the repo-list load and the git-status pass would race
    /// the first paint.
    /// </summary>
    private void OnMainWindowOpened(object? sender, EventArgs e)
    {
        Opened -= OnMainWindowOpened;
        if (ContentArea.Content is ReposPage { DataContext: ReposViewModel viewModel } page)
        {
            FireLifecycle(() => viewModel.OnNavigatedToAsync());
            FireLifecycle(page.BottomBarViewModel.InitializeAsync);
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
        // narrows the drawer (dragging left widens — the card is docked right). The max
        // is the window's live client width minus the card's 12+12 outer gutters, read
        // per drag tick so a resized window re-bounds the drag immediately.
        PanelResizeController.Attach(
            ToolDrawerResizer,
            this,
            delta => ViewModel.AdjustToolDrawerWidth(delta, ClientSize.Width - 24),
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

        _globalSearch.PropertyChanged -= OnGlobalSearchPropertyChanged;
        // An open drawer hosts a transient ViewModel subscribed to singleton services;
        // Close() routes the teardown through OnToolDrawerChanged so the VM is not
        // rooted by them for the remaining process lifetime.
        _toolDrawer.Close();
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
        // services in OnNavigatedFromAsync, and plain-ObservableObject tool components
        // (the clone drawer) cancel their in-flight work in OnDrawerClosed.
        if (ToolDrawerHost.Content is Control previous)
        {
            if (previous.DataContext is PageViewModelBase outgoingVm)
            {
                FireLifecycle(() => outgoingVm.OnNavigatedFromAsync());
            }
            if (previous.DataContext is IToolDrawerTeardown outgoingTeardown)
            {
                FireLifecycle(() =>
                {
                    outgoingTeardown.OnDrawerClosed();
                    return Task.CompletedTask;
                });
            }
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
                case IToolDrawerContextReceiver<NewBranchContext> newBranch:
                    await newBranch.OnDrawerContextAsync(context as NewBranchContext);
                    break;
                case IToolDrawerContextReceiver<CloneFromUrlContext> cloneFromUrl:
                    await cloneFromUrl.OnDrawerContextAsync(context as CloneFromUrlContext);
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
    /// logger instead of silently swallowing them. Hooks dispatch on the UI thread —
    /// a thread-pool run would let teardown race the next instance's constructor
    /// subscribe and let view-models mutate observable state off the UI thread.
    /// Fire-and-forget mirrors the former navigation service's handling of the same
    /// hooks.
    /// </summary>
    private static void FireLifecycle(Func<Task> hook)
    {
        _ = Dispatcher.UIThread.InvokeAsync(async () =>
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
    /// Typing a character anywhere outside a text field starts a repo search: the
    /// header search box takes focus and the character becomes the fresh term.
    /// TextInput bubbles only when no text-consuming control wanted the key — a
    /// focused TextBox (commit message, tag flyout, the search field itself) handles
    /// its text input first and never gets hijacked, and modifier-only shortcuts
    /// produce no text at all.
    /// </summary>
    protected override void OnTextInput(TextInputEventArgs e)
    {
        if (!e.Handled
            && HeaderSearchBox is not null
            && !HeaderSearchBox.IsFocused
            && !string.IsNullOrEmpty(e.Text))
        {
            HeaderSearchBox.Focus();
            HeaderSearchBox.Text = e.Text;
            HeaderSearchBox.CaretIndex = e.Text.Length;
            e.Handled = true;
        }
        base.OnTextInput(e);
    }

    /// <summary>
    /// Header search typed text: debounce a global search (repositories + notes). An
    /// empty field resets the dropdown content.
    /// </summary>
    private void OnHeaderSearchTextChanged(object? sender, TextChangedEventArgs e)
    {
        UpdateHeaderSearchChrome();
        _searchDebounce.Debounce(() => _ = RunHeaderSearchAsync(HeaderSearchBox?.Text ?? string.Empty));
    }

    /// <summary>
    /// Arrow keys move the dropdown's highlight (reopening a light-dismissed popup with
    /// results still loaded), Enter activates the highlighted or first result, Escape
    /// closes the dropdown first and clears the field only once it is already closed.
    /// </summary>
    private void OnHeaderSearchKeyDown(object? sender, KeyEventArgs e)
    {
        switch (e.Key)
        {
            case Key.Down when _globalSearch.Results.Count > 0:
                if (HeaderSearchPopup is { IsOpen: false } closedPopup)
                {
                    closedPopup.IsOpen = true;
                }
                if (HeaderSearchResultsList is { } downList)
                {
                    downList.SelectedIndex = Math.Min(downList.SelectedIndex + 1, _globalSearch.Results.Count - 1);
                }
                e.Handled = true;
                break;
            case Key.Up when _globalSearch.Results.Count > 0:
                if (HeaderSearchPopup is { IsOpen: false } reopenedPopup)
                {
                    reopenedPopup.IsOpen = true;
                }
                if (HeaderSearchResultsList is { } upList)
                {
                    upList.SelectedIndex = Math.Max(upList.SelectedIndex - 1, 0);
                }
                e.Handled = true;
                break;
            case Key.Enter:
                _searchDebounce.Cancel();
                ActivateHighlightedSearchResult();
                e.Handled = true;
                break;
            case Key.Escape:
                if (HeaderSearchPopup is { IsOpen: true } openPopup)
                {
                    openPopup.IsOpen = false;
                }
                else if (HeaderSearchBox is not null)
                {
                    HeaderSearchBox.Text = string.Empty;
                }
                e.Handled = true;
                break;
        }
    }

    private void OnSearchClearClick(object? sender, RoutedEventArgs e)
    {
        if (HeaderSearchBox is not null) HeaderSearchBox.Text = string.Empty;
    }

    /// <summary>Mouse activation: the tapped row's item (walked up from the deepest
    /// visual) — programmatic SelectedIndex writes from the arrow keys never route
    /// through pointer events, so the two activation paths cannot collide.</summary>
    private void OnSearchResultTapped(object? sender, TappedEventArgs e)
    {
        if ((e.Source as Control)?.FindAncestorOfType<ListBoxItem>(includeSelf: true)?.DataContext
            is GlobalSearchResultViewModel result)
        {
            ActivateSearchResult(result);
        }
    }

    /// <summary>Runs one search; the popup open-state re-syncs from the ViewModel's
    /// result pushes (see <see cref="OnGlobalSearchPropertyChanged"/>), so this only
    /// needs to cover the empty-term reset.</summary>
    private async Task RunHeaderSearchAsync(string text)
    {
        if (string.IsNullOrWhiteSpace(text))
        {
            _globalSearch.Reset();
            SyncHeaderSearchPopup();
            return;
        }

        await _globalSearch.SearchAsync(text);
        SyncHeaderSearchPopup();
    }

    /// <summary>The dropdown shows while the field holds a term, the latest search has
    /// something to say (results or the no-match footer), and the field still owns
    /// focus — a search finishing after a light dismiss must not pop the list open.</summary>
    private void SyncHeaderSearchPopup()
    {
        if (HeaderSearchPopup is null || HeaderSearchBox is null)
        {
            return;
        }

        var hasText = !string.IsNullOrWhiteSpace(HeaderSearchBox.Text);
        var hasContent = _globalSearch.Results.Count > 0 || _globalSearch.HasNoResults;
        HeaderSearchPopup.IsOpen = hasText && hasContent && HeaderSearchBox.IsFocused;
    }

    /// <summary>Result pushes from the ViewModel: re-sync the popup and highlight the
    /// top hit. The highlight lands posted — the ItemsSource swap resets the ListBox's
    /// selection asynchronously (the notes tree restore works around the same echo).</summary>
    private void OnGlobalSearchPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is not (nameof(GlobalSearchViewModel.Results) or nameof(GlobalSearchViewModel.HasNoResults)))
        {
            return;
        }

        SyncHeaderSearchPopup();
        Dispatcher.UIThread.Post(
            () =>
            {
                if (HeaderSearchResultsList is { } list
                    && list.SelectedIndex < 0
                    && _globalSearch.Results.Count > 0)
                {
                    list.SelectedIndex = 0;
                }
            },
            DispatcherPriority.ApplicationIdle);
    }

    private void ActivateHighlightedSearchResult()
    {
        var result = HeaderSearchResultsList?.SelectedItem as GlobalSearchResultViewModel
            ?? (_globalSearch.Results.Count > 0 ? _globalSearch.Results[0] : null);
        if (result is not null)
        {
            ActivateSearchResult(result);
        }
    }

    /// <summary>Consumes a picked row: closes the dropdown and clears the field (the
    /// clear schedules an empty-term reset, cancelled right after), then runs the
    /// row's action.</summary>
    private void ActivateSearchResult(GlobalSearchResultViewModel result)
    {
        if (HeaderSearchPopup is { } popup)
        {
            popup.IsOpen = false;
        }
        if (HeaderSearchBox is not null)
        {
            HeaderSearchBox.Text = string.Empty;
        }
        _searchDebounce.Cancel();

        if (result is { Kind: GlobalSearchResultKind.Repo, Repo: { } repo })
        {
            ActivateRepoResult(repo);
        }
        else if (result is { Kind: GlobalSearchResultKind.Note, Hit: { } hit })
        {
            _ = ActivateNoteResultAsync(hit);
        }
    }

    /// <summary>Repo activation: the Repositories page returns (if another page was
    /// showing), the repo is selected like a row press, and the row scrolls into view.</summary>
    private void ActivateRepoResult(Repo repo)
    {
        if (ContentArea.Content is not ReposPage)
        {
            ContentArea.Content = _reposPage;
        }

        _globalSearch.SelectRepo(repo);
        _reposPage.RevealRepo(repo);
    }

    /// <summary>Note activation: the note's repo is selected first (its notes folder
    /// auto-expands in the tree), then the Notes page navigates with the hit as its
    /// pending-open note. Repos the app does not track still open the page.</summary>
    private async Task ActivateNoteResultAsync(NotesSearchHit hit)
    {
        var repo = _globalSearch.ResolveRepoForHit(hit);
        if (repo is not null)
        {
            _globalSearch.SelectRepo(repo);
        }

        if (ContentArea.Content is not NotesPage)
        {
            ContentArea.Content = _notesPage;
        }

        if (_notesPage.DataContext is NotesPageViewModel viewModel)
        {
            try
            {
                await viewModel.NavigateToNoteAsync(hit);
            }
            catch (Exception ex)
            {
                Serilog.Log.Logger.Error(ex, "Global search failed to open note {Path}", hit.FullPath);
            }
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
