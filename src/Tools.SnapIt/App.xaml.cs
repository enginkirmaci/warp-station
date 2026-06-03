using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Microsoft.Extensions.DependencyInjection;
using Tools.SnapIt.Applications;
using Tools.SnapIt.Contracts;
using Tools.SnapIt.Services;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt;

public partial class App : Application
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

	public override void OnFrameworkInitializationCompleted()
	{
		if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
		{
			desktop.Startup += OnStartup;
			desktop.Exit += OnExit;
		}
		base.OnFrameworkInitializationCompleted();
	}

	private async void OnStartup(object sender, ControlledApplicationLifetimeStartupEventArgs e)
	{
		try
		{
			startupArgs = e.Args?.ToArray() ?? [];

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
						ShutdownApp();
					}
					}
				else if (!Dev.IsActive)
				{
					ShutdownApp();
					AppLauncher.RunAsAdmin();
				}
				}
				else
				{
				if (!AppInstance.RegisterSingleInstance() && !Dev.IsActive)
				{
					ShutdownApp();
				}
				}
			}
		}
		catch (Exception ex)
		{
			Dev.Log($"Startup error: {ex}");
			ShutdownApp();
		}
	}

	private void ShutdownApp()
	{
		if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime lifetime)
		{
			lifetime.Shutdown();
		}
	}

	private void OnExit(object sender, ControlledApplicationLifetimeExitEventArgs e)
	{
		(_services as IDisposable)?.Dispose();
	}
}
