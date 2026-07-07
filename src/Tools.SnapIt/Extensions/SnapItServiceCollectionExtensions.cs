using Microsoft.Extensions.DependencyInjection;
using Tools.SnapIt.Contracts;
using Tools.SnapIt.Services;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Extensions;

/// <summary>
/// Registers the SnapIt engine (manager, window/screen services, hooks) with the DI container.
/// </summary>
public static class SnapItServiceCollectionExtensions
{
    public static IServiceCollection AddSnapItEngine(this IServiceCollection services)
    {
        services.AddSingleton<ISnapManager, SnapManager>();
        services.AddSingleton<IWindowManager, WindowManager>();
        services.AddSingleton<IScreenManager, ScreenManager>();

        services.AddSingleton<IGlobalHookService, GlobalHookService>();
        services.AddSingleton<IMouseService, MouseService>();
        services.AddSingleton<IFileOperationService, FileOperationService>();
        services.AddSingleton<ISettingService, SettingService>();
        services.AddSingleton<IWinApiService, WinApiService>();
        services.AddSingleton<IWindowsService, WindowsService>();
        services.AddSingleton<IWindowEventService, WindowEventService>();

        return services;
    }
}
