using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Serilog;
using SukiUI;
using Tools.Helpers;
using Tools.Library.Extensions;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.SnapIt.Extensions;
using Tools.Services;
using Tools.Services.Abstractions;
using Tools.ViewModels.Pages;
using Tools.ViewModels.Windows;
using Tools.Views.Components;
using Tools.Views.Components.Git;
using Tools.Views.Components.Repo;
using Tools.Views.Components.Settings;
using Tools.Views.Components.Tools;
using Tools.Views.Pages;
using Tools.Views.Windows;

namespace Tools;

/// <summary>
/// Provides application-specific behavior to supplement the default Application class.
/// </summary>
public partial class App : Application
{
    public static IHost Host { get; private set; } = null!;

    private MainWindow? _mainWindow;

    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
        // SukiTheme (ThemeColor="Blue") writes its SukiPrimaryColor* palette into
        // Application.Resources while App.xaml loads, so the override must land
        // after AvaloniaXamlLoader.Load to win.
        ApplyAccentColorOverride();
        // Re-apply whenever SukiTheme rewrites the palette (theme color / light-dark change),
        // so the custom accent survives runtime theme switches.
        if (Styles.OfType<SukiTheme>().FirstOrDefault() is { } sukiTheme)
            sukiTheme.OnColorThemeChanged += _ => ApplyAccentColorOverride();
        // Configure Serilog: Error-level to the daily rolling file.
        var logFilePath = Path.Combine(AppContext.BaseDirectory, "logs", "log.txt");
        Log.Logger = new LoggerConfiguration()
            .WriteToFileDaily(logFilePath)
            .CreateLogger();
        // Configure icon asset loader to resolve SVG icons from this assembly
        IconAssetLoader.Configure(typeof(App).Assembly.GetName().Name ?? "Tools");
        IconAssetLoader.PreloadAll();
        // Build the host with DI
        Host = Microsoft.Extensions.Hosting.Host.CreateDefaultBuilder()
            .ConfigureServices((context, services) =>
            {
                ConfigureServices(services);
            })
            .Build();
        Log.Logger.Information("Dev Tools Started");
    }

    /// <summary>
    /// Overrides SukiTheme's primary palette with the app accent: the soft periwinkle
    /// #B5CDFC the Outlined buttons previously rendered in dark mode (SukiUI Lightens its
    /// Blue primary #0A59F7 by 0.7 for SukiPrimaryColor120). The vivid raw primary becomes
    /// that periwinkle, and SukiPrimaryColor120 is pinned to it so Outlined buttons keep
    /// rendering exactly as before; the remaining variants mirror SukiTheme's
    /// SetColorWithOpacities / PrimaryDark derivation off the same base color.
    /// </summary>
    private void ApplyAccentColorOverride()
    {
        var accent = Color.Parse("#7090cf");
        SetAccentResource("SukiPrimaryColor", accent);
        SetAccentResource("SukiPrimaryColor75", accent, 0.75);
        SetAccentResource("SukiPrimaryColor50", accent, 0.50);
        SetAccentResource("SukiPrimaryColor25", accent, 0.25);
        SetAccentResource("SukiPrimaryColor20", accent, 0.2);
        SetAccentResource("SukiPrimaryColor15", accent, 0.15);
        SetAccentResource("SukiPrimaryColor10", accent, 0.10);
        SetAccentResource("SukiPrimaryColor7", accent, 0.07);
        SetAccentResource("SukiPrimaryColor5", accent, 0.05);
        SetAccentResource("SukiPrimaryColor3", accent, 0.03);
        SetAccentResource("SukiPrimaryColor1", accent, 0.005);
        SetAccentResource("SukiPrimaryColor0", accent, 0.00);
        Resources["SukiPrimaryColor120"] = accent;
        // Lighten(accent, 1) = white in dark mode, same as SukiTheme's dark branch
        Resources["SukiPrimaryColor150"] = SukiTheme.Lighten(accent, 1);
        // PrimaryDark halves each channel, as SukiColorTheme does
        Resources["SukiPrimaryDarkColor"] = new Color(255, (byte)(accent.R / 2), (byte)(accent.G / 2), (byte)(accent.B / 2));
    }

    // Same derivation as SukiTheme.SetColorWithOpacities (ColorExtensions.WithAlpha truncates)
    private void SetAccentResource(string key, Color accent, double alpha = 1.0) =>
        Resources[key] = new Color((byte)(255 * alpha), accent.R, accent.G, accent.B);

    private void ConfigureServices(IServiceCollection services)
    {
        // Register core library services
        services.AddCoreServices();
        // Register the SnapIt engine and its in-process host adapter (ISnapItService)
        services.AddSnapItEngine();
        services.AddSingleton<ISnapItService, SnapItService>();
        // Register application services
        services.AddSingleton<IToolDrawerService, ToolDrawerService>();
        // Main-window access for services (clipboard, folder picker): resolves the
        // desktop lifetime's MainWindow at call time, so services never depend on the
        // window view and the DI graph stays cycle-free.
        services.AddSingleton<IMainWindowProvider, MainWindowProvider>();
        services.AddSingleton<IClipboardPasswordService, ClipboardPasswordService>();
        services.AddSingleton<IDialogService, DialogService>();
        services.AddSingleton<INotificationService, NotificationService>();
        services.AddSingleton<IOpenCodeGridLauncher, OpenCodeGridLauncher>();
        // Register windows and view models
        services.AddSingleton<MainWindow>();
        services.AddSingleton<MainWindowViewModel>();
        // Singleton: the bottom bar outlives page navigation and keeps its state (repo
        // selection, open tab) for the whole window lifetime.
        services.AddSingleton<ViewModels.Components.BottomBar.BottomBarViewModel>();
        // Register pages and view models
        // The Repositories page is permanent window content (attached once in the
        // MainWindow constructor), so its ViewModel is a Singleton: the constructor
        // subscribes to singleton services and its detach path can never run — a
        // Transient registration would leak a subscribed VM per future resolution.
        services.AddSingleton<ReposViewModel>();
        services.AddTransient<ReposPage>();
        // Register tool components (floating drawer) and their view models
        RegisterPageWithViewModel<FormattersComponent, FormattersViewModel>(services);
        RegisterPageWithViewModel<NugetLocalComponent, NugetLocalViewModel>(services);
        RegisterPageWithViewModel<CodeExecuteComponent, CodeExecuteViewModel>(services);
        RegisterPageWithViewModel<ClipboardPasswordComponent, ClipboardPasswordViewModel>(services);
        RegisterPageWithViewModel<SnapItSettingsComponent, SnapItSettingsViewModel>(services);
        RegisterPageWithViewModel<OpenCodeSettingsComponent, OpenCodeSettingsViewModel>(services);
        // Drawer-hosted dialogs (opened by DialogService instead of modal windows)
        RegisterPageWithViewModel<AddRepositoryComponent, AddRepositoryViewModel>(services);
        RegisterPageWithViewModel<ReposSettingsComponent, ReposSettingsViewModel>(services);
        RegisterPageWithViewModel<NewBranchComponent, NewBranchViewModel>(services);
        RegisterPageWithViewModel<CloneFromUrlComponent, CloneFromUrlViewModel>(services);
        // Drawer-component factory: MainWindow resolves drawer views through this
        // intention-revealing delegate instead of holding the service container.
        // Resolution matches the former GetService(ViewType) exactly — a fresh
        // transient view (with its transient ViewModel) per drawer open.
        services.AddSingleton<ToolViewResolver>(sp => key =>
            ToolComponentMapper.Find(key)?.ViewType is { } viewType
                ? sp.GetService(viewType) as Control
                : null);
    }

    private static void RegisterPageWithViewModel<TPage, TViewModel>(IServiceCollection services)
        where TPage : class
        where TViewModel : class
    {
        services.AddTransient<TPage>();
        services.AddTransient<TViewModel>();
    }

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var services = Host.Services;
            _mainWindow = services.GetRequiredService<MainWindow>();
            desktop.MainWindow = _mainWindow;
            desktop.ShutdownMode = ShutdownMode.OnMainWindowClose;
            // Stop background services and dispose the host on application shutdown,
            // so their lifecycle is owned here rather than by a window close handler.
            desktop.ShutdownRequested += OnShutdownRequested;
            // Start minimized to the taskbar if configured
            _ = ApplyStartMinimizedAsync(services, _mainWindow);
            // Reconcile the sign-in registration with the configured StartAtBoot flag
            // (honors hand-edited settings.json and repairs stale registrations even
            // when the supervisor never runs, e.g. the AppImage layout)
            _ = SyncStartAtBootAsync(services);
            // Auto-start SnapIt if configured
            _ = InitializeSnapItAsync(services);
        }
        base.OnFrameworkInitializationCompleted();
    }

    private static void OnShutdownRequested(object? sender, ShutdownRequestedEventArgs e)
    {
        var services = Host.Services;
        try
        {
            // Cancelling makes the run service kill the opencode process tree on this
            // thread — without it a mid-generation CLI outlives the closed app.
            services.GetRequiredService<ViewModels.Components.BottomBar.BottomBarViewModel>()
                .CancelCommitMessageGeneration();
            services.GetRequiredService<ISnapItService>().Stop();
            services.GetRequiredService<INugetLocalService>().Stop();
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to stop background services on shutdown");
        }
        finally
        {
            Host.Dispose();
        }
    }

    private static async Task InitializeSnapItAsync(IServiceProvider services)
    {
        // SnapIt is Windows-only functionality (Win32 engine); never start it elsewhere.
        if (!OperatingSystem.IsWindows())
        {
            return;
        }

        try
        {
            var settingsService = services.GetRequiredService<ISettingsService>();
            var appSettings = await settingsService.GetSettingsAsync();
            if (appSettings.SnapIt?.AutoStart == true)
            {
                var snapItService = services.GetRequiredService<ISnapItService>();
                await snapItService.StartAsync();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to auto-start SnapIt");
        }
    }

    private static async Task ApplyStartMinimizedAsync(IServiceProvider services, Window mainWindow)
    {
        try
        {
            var settingsService = services.GetRequiredService<ISettingsService>();
            var appSettings = await settingsService.GetSettingsAsync();
            if (appSettings.General?.StartMinimized == true)
            {
                mainWindow.WindowState = WindowState.Minimized;
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to apply start minimized");
        }
    }

    /// <summary>
    /// Mirrors the supervisor's launch reconcile: registers or clears the OS sign-in
    /// entry (registry Run key on Windows, XDG autostart elsewhere) to match
    /// General.StartAtBoot, targeting <see cref="Tools.Library.Helpers.AutoStartHelper.ResolveBootTarget"/>.
    /// </summary>
    private static async Task SyncStartAtBootAsync(IServiceProvider services)
    {
        try
        {
            var settingsService = services.GetRequiredService<ISettingsService>();
            var appSettings = await settingsService.GetSettingsAsync();
            var startAtBoot = appSettings.General?.StartAtBoot == true;
            Tools.Library.Helpers.AutoStartHelper.Sync(startAtBoot, Tools.Library.Helpers.AutoStartHelper.ResolveBootTarget());
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to sync the launch-at-sign-in registration");
        }
    }
}