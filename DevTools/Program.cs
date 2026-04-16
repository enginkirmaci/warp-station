using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Serilog;
using Tools.Library.Extensions;
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