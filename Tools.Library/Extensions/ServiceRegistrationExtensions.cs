using Microsoft.Extensions.DependencyInjection;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

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

        return services;
    }
}
