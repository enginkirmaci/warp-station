using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Serilog;
using Tools.Library.Extensions;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.ViewModels.Pages;
using Tools.ViewModels.Windows;
using Tools.Views.Pages;
using Tools.Views.Windows;

namespace Tools;

/// <summary>
/// Provides application-specific behavior to supplement the default Application class.
/// </summary>
public partial class App : Application
{
    public static IHost Host { get; private set; } = null!;
    public static IServiceProvider Services => Host.Services;

    private Window? _mainWindow;

    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
        // Configure Serilog
        Log.Logger = new LoggerConfiguration()
            .MinimumLevel.Error()
            .WriteTo.File("logs\\log.txt", rollingInterval: RollingInterval.Day)
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

    private static void ConfigureServices(IServiceCollection services)
    {
        // Register core library services
        services.AddCoreServices();
        // Register application services
        services.AddSingleton<INavigationService, NavigationService>();
        services.AddSingleton<IClipboardPasswordService, ClipboardPasswordService>();
        // Register Focus Timer service
        services.AddSingleton<IFocusTimerService, FocusTimerService>();
        // Register windows and view models
        services.AddSingleton<MainWindow>();
        services.AddSingleton<MainWindowViewModel>();
        // Register Timer Notification Window (singleton to reuse)
        services.AddSingleton<TimerNotificationWindow>();
        services.AddSingleton<TimerNotificationWindowViewModel>();
        // Register pages and view models
        RegisterPageWithViewModel<DashboardPage, DashboardViewModel>(services);
        RegisterPageWithViewModel<WorkspacesPage, WorkspacesViewModel>(services);
        RegisterPageWithViewModel<FormattersPage, FormattersPageViewModel>(services);
        RegisterPageWithViewModel<NugetLocalPage, NugetLocalViewModel>(services);
        RegisterPageWithViewModel<EFToolsPage, EFToolsPageViewModel>(services);
        RegisterPageWithViewModel<CodeExecutePage, CodeExecutePageViewModel>(services);
        RegisterPageWithViewModel<ClipboardPasswordPage, ClipboardPasswordPageViewModel>(services);
        RegisterPageWithViewModel<FocusTimerSettingsPage, FocusTimerSettingsViewModel>(services);
        RegisterPageWithViewModel<SnapItSettingsPage, SnapItSettingsViewModel>(services);
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
            _mainWindow = Services.GetRequiredService<MainWindow>();
            desktop.MainWindow = _mainWindow;
            desktop.ShutdownMode = ShutdownMode.OnMainWindowClose;
            // Start minimized to the taskbar if configured
            _ = ApplyStartMinimizedAsync(_mainWindow);
            // Auto-start SnapIt if configured
            _ = InitializeSnapItAsync();
        }
        base.OnFrameworkInitializationCompleted();
    }

    public static TService GetService<TService>() where TService : class
    {
        return Services.GetRequiredService<TService>();
    }

    public static Window MainWindow => ((App)Current!)._mainWindow!;

    private static async Task InitializeSnapItAsync()
    {
        try
        {
            var settingsService = Services.GetRequiredService<ISettingsService>();
            var appSettings = await settingsService.GetSettingsAsync();
            if (appSettings.SnapIt?.AutoStart == true)
            {
                var snapItService = Services.GetRequiredService<ISnapItService>();
                await snapItService.StartAsync();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to auto-start SnapIt");
        }
    }

    private static async Task ApplyStartMinimizedAsync(Window mainWindow)
    {
        try
        {
            var settingsService = Services.GetRequiredService<ISettingsService>();
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
}