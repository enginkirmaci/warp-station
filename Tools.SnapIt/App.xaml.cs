using Microsoft.Extensions.DependencyInjection;
using Tools.SnapIt.Applications;
using Tools.SnapIt.Contracts;
using Tools.SnapIt.Services;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt;

public partial class App
{
	public string[] startupArgs;

	private static readonly IServiceProvider _services = ConfigureServices();

	private static IServiceProvider ConfigureServices()
	{
		var services = new ServiceCollection();

		_ = services.AddSingleton<ISnapManager, SnapManager>();
		_ = services.AddSingleton<IWindowManager, WindowManager>();
		_ = services.AddSingleton<IScreenManager, ScreenManager>();

		_ = services.AddSingleton<IGlobalHookService, GlobalHookService>();
		_ = services.AddSingleton<IMouseService, MouseService>();
		_ = services.AddSingleton<IFileOperationService, FileOperationService>();
		_ = services.AddSingleton<ISettingService, SettingService>();
		_ = services.AddSingleton<IWinApiService, WinApiService>();
		_ = services.AddSingleton<IWindowsService, WindowsService>();
		_ = services.AddSingleton<IWindowEventService, WindowEventService>();

		return services.BuildServiceProvider();
	}

	public static IServiceProvider Services => _services;

	private async void OnStartup(object sender, StartupEventArgs e)
	{
		try
		{
			startupArgs = e.Args;

			var snapManager = Services.GetRequiredService<ISnapManager>();
			await snapManager.InitializeAsync();

			var settingService = Services.GetRequiredService<ISettingService>();
			await settingService.LoadSettingsAsync();

			if (!AppLauncher.BypassSingleInstance(startupArgs))
			{
				if (settingService.Settings.RunAsAdmin && !Dev.SkipRunAsAdmin)
				{
					if (AppLauncher.IsAdmin(startupArgs))
					{
						if (!AppInstance.RegisterSingleInstance())
						{
							Shutdown();
						}
					}
					else if (!Dev.IsActive)
					{
						Shutdown();
						AppLauncher.RunAsAdmin();
					}
				}
				else
				{
					if (!AppInstance.RegisterSingleInstance() && !Dev.IsActive)
					{
						Shutdown();
					}
				}
			}
		}
		catch (Exception ex)
		{
			Dev.Log($"Startup error: {ex}");
			Shutdown();
		}
	}

	private void OnExit(object sender, ExitEventArgs e)
	{
		(_services as IDisposable)?.Dispose();
	}
}