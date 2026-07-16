using Microsoft.Extensions.DependencyInjection;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Library.Services.OpenCode;

namespace Tools.Library.Extensions;

/// <summary>
/// Extension methods for service registration.
/// </summary>
public static class ServiceRegistrationExtensions
{
    /// <summary>
    /// Registers core library services.
    /// </summary>
    /// <param name="services">The service collection.</param>
    /// <returns>The service collection for chaining.</returns>
    public static IServiceCollection AddCoreServices(this IServiceCollection services)
    {
        services.AddSingleton<ISettingsService, SettingsService>();
        services.AddSingleton<IClipboardService, ClipboardService>();
        services.AddSingleton<INugetLocalService, NugetLocalService>();
        services.AddSingleton<IOpenCodeTemplateService, OpenCodeTemplateService>();
        services.AddSingleton<IOpenCodePromptService, OpenCodePromptService>();

        // Repos subsystem: scanner, cache store, and the singleton orchestrator
        services.AddSingleton<IRepoScanner, RepoScanner>();
        services.AddSingleton<IRepoCacheStore, RepoCacheStore>();
        services.AddSingleton<IRepoService, RepoService>();

        // Process launcher and DevTools client for IPC
        services.AddSingleton<IProcessLauncher, ProcessLauncher>();
        services.AddSingleton<IDevToolsClient, DevToolsClient>();

        // opencode serve integration: HTTP/SSE client + managed subprocess service
        services.AddSingleton<IOpenCodeServeClient, OpenCodeServeClient>();
        services.AddSingleton<IOpenCodeServeService, OpenCodeServeService>();

        return services;
    }
}
