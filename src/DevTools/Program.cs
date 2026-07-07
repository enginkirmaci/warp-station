using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Serilog;
using Tools.Library.Extensions;
using Tools.Library.Services.Abstractions;
using DevTools.Services;

Log.Logger = new LoggerConfiguration()
    .MinimumLevel.Information()
    .WriteTo.Console()
    .CreateLogger();

Log.Information("DevTools starting...");

// Build the host with DI
var host = Microsoft.Extensions.Hosting.Host.CreateDefaultBuilder()
    .ConfigureServices((context, services) =>
    {
        // Register core library services
        services.AddCoreServices();

        // Register DevTools services
        services.AddSingleton<NamedPipeServer>();
        services.AddSingleton<DevToolsService>();
    })
    .Build();

var devToolsService = host.Services.GetRequiredService<DevToolsService>();

// Reconcile the launch-at-sign-in registration (Windows only)
await SyncStartAtBootAsync(host.Services);

// Handle Ctrl+C and graceful shutdown
Console.CancelKeyPress += (sender, e) =>
{
    e.Cancel = true;
    Log.Information("Shutting down DevTools...");
    devToolsService.Stop();
};

// Get application lifetime to handle shutdown
var lifetime = host.Services.GetRequiredService<IHostApplicationLifetime>();

// Start the service
await devToolsService.StartAsync();

// Keep running until shutdown
Log.Information("DevTools is running. Press Ctrl+C to stop.");
await Task.Delay(Timeout.Infinite, lifetime.ApplicationStopping);

Log.Information("DevTools stopped");

// Reconciles the Windows registry Run key with the configured StartAtBoot flag,
// so settings.json remains the single source of truth for launch-at-sign-in.
async Task SyncStartAtBootAsync(IServiceProvider services)
{
    try
    {
        var settingsService = services.GetRequiredService<ISettingsService>();
        var appSettings = await settingsService.GetSettingsAsync();
        var startAtBoot = appSettings.General?.StartAtBoot == true;
#if WINDOWS
        DevTools.Helpers.AutoStartHelper.Sync(startAtBoot);
#endif
    }
    catch (Exception ex)
    {
        Log.Error(ex, "Failed to sync start-at-boot registration");
    }
}