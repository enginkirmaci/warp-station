using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Serilog;
using Tools.Library.Extensions;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.SnapIt.Extensions;
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

    private Window? _mainWindow;

    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
        // Configure Serilog using the shared bootstrap configuration
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

    private static void ConfigureServices(IServiceCollection services)
    {
        // Register core library services
        services.AddCoreServices();
        // Register the SnapIt engine and its in-process host adapter (ISnapItService)
        services.AddSnapItEngine();
        services.AddSingleton<ISnapItService, SnapItService>();
        // Register application services
        services.AddSingleton<INavigationService, NavigationService>();
        services.AddSingleton<IClipboardPasswordService, ClipboardPasswordService>();
        services.AddSingleton<IDialogService, DialogService>();
        // Register windows and view models
        services.AddSingleton<MainWindow>();
        services.AddSingleton<MainWindowViewModel>();
        // Register pages and view models
        RegisterPageWithViewModel<DashboardPage, DashboardViewModel>(services);
        RegisterPageWithViewModel<ReposPage, ReposViewModel>(services);
        RegisterPageWithViewModel<FormattersPage, FormattersPageViewModel>(services);
        RegisterPageWithViewModel<NugetLocalPage, NugetLocalViewModel>(services);
        RegisterPageWithViewModel<EFToolsPage, EFToolsPageViewModel>(services);
        RegisterPageWithViewModel<CodeExecutePage, CodeExecutePageViewModel>(services);
        RegisterPageWithViewModel<ClipboardPasswordPage, ClipboardPasswordPageViewModel>(services);
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
            var services = Host.Services;
            _mainWindow = services.GetRequiredService<MainWindow>();
            desktop.MainWindow = _mainWindow;
            desktop.ShutdownMode = ShutdownMode.OnMainWindowClose;
            // Stop background services and dispose the host on application shutdown,
            // so their lifecycle is owned here rather than by a window close handler.
            desktop.ShutdownRequested += OnShutdownRequested;
            // Start minimized to the taskbar if configured
            _ = ApplyStartMinimizedAsync(services, _mainWindow);
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
}