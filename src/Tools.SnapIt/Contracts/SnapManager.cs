using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Contracts;

public class SnapManager : ISnapManager
{
	private readonly IWindowManager windowManager;
	private readonly ISettingService settingService;
	private readonly IWinApiService winApiService;
	private readonly IScreenManager screenManager;
	private readonly IMouseService mouseService;
	private readonly IWindowsService windowsService;
	private readonly IWindowEventService windowEventService;

	public bool IsInitialized { get; private set; }
	public bool IsRunning { get; set; }

	public event GetStatus StatusChanged;

	public event ScreenChangedEvent ScreenChanged;

	public event LayoutChangedEvent LayoutChanged;

	public event ScreenLayoutLoadedEvent ScreenLayoutLoaded;

	public SnapManager(
		IWindowManager windowManager,
		ISettingService settingService,
		IWinApiService winApiService,
		IScreenManager screenManager,
		IMouseService mouseService,
		IWindowsService windowsService,
		IWindowEventService windowEventService)
	{
		this.windowManager = windowManager;
		this.settingService = settingService;
		this.winApiService = winApiService;
		this.screenManager = screenManager;
		this.mouseService = mouseService;
		this.windowsService = windowsService;
		this.windowEventService = windowEventService;
	}

	public async Task InitializeAsync()
	{
		if (IsInitialized)
		{
			return;
		}

		await windowManager.InitializeAsync();
		await screenManager.InitializeAsync();
		await winApiService.InitializeAsync();
		await settingService.InitializeAsync();
		await mouseService.InitializeAsync();
		await windowsService.InitializeAsync();
		await windowEventService.InitializeAsync();
		windowEventService.StartMonitoring();

		if (Dev.ShowSnapWindowOnStartup)
		{
			windowManager.Show();
		}

		screenManager.SetSnapManager(this);

		mouseService.MoveWindow += MoveWindow;
		mouseService.SnappingCancelled += SnappingCancelled;

		IsRunning = true;
		StatusChanged?.Invoke(true);
		ScreenLayoutLoaded?.Invoke(settingService.SnapScreens, settingService.Layouts);

		IsInitialized = true;
	}

	public void StartStop()
	{
		if (IsRunning)
		{
			Dispose();
		}
		else
		{
			_ = InitializeAsync();
		}
	}

	private void MoveWindow(SnapAreaInfo snapAreaInfo, bool isLeftClick)
	{
		MoveWindow(snapAreaInfo.ActiveWindow, snapAreaInfo.Rectangle, isLeftClick);
	}

	private void SnappingCancelled()
	{
		Dispatcher.UIThread.Post(() => windowManager.Hide());
		mouseService.Interrupt();
	}

	public void Dispose()
	{
		windowManager.Dispose();

		mouseService.MoveWindow -= MoveWindow;
		mouseService.SnappingCancelled -= SnappingCancelled;
		mouseService.Dispose();

		screenManager.Dispose();
		winApiService.Dispose();
		settingService.Dispose();
		windowsService.Dispose();
		windowEventService.Dispose();

		IsRunning = false;
		StatusChanged?.Invoke(false);
		IsInitialized = false;
	}

	public void ScreenChangedEvent()
	{
		settingService.ReInitialize();

		if (IsRunning)
		{
			Dispose();
			_ = InitializeAsync();
		}

		ScreenChanged?.Invoke(settingService.SnapScreens);
	}

	private void MoveWindow(ActiveWindow currentWindow, Rectangle rectangle, bool isLeftClick)
	{
		if (currentWindow != ActiveWindow.Empty)
		{
			if (rectangle != null && !rectangle.IsEmpty)
			{
				winApiService.GetWindowMargin(currentWindow, out Rectangle withMargin);

				if (!withMargin.IsEmpty)
				{
					var marginHorizontal = (currentWindow.Boundry.Width - withMargin.Width) / 2;
					var systemMargin = new Rectangle
					{
						Left = marginHorizontal,
						Right = marginHorizontal,
						Top = 0,
						Bottom = currentWindow.Boundry.Height - withMargin.Height
					};

					rectangle.Left -= systemMargin.Left;
					rectangle.Top -= systemMargin.Top;
					rectangle.Right += systemMargin.Right;
					rectangle.Bottom += systemMargin.Bottom;
				}

				if (isLeftClick)
				{
					_ = Task.Run(async () =>
					{
						await Task.Delay(100);
						winApiService.MoveWindow(currentWindow, rectangle);

						if (!rectangle.Dpi.Equals(currentWindow?.Dpi))
						{
							winApiService.MoveWindow(currentWindow, rectangle);
						}
					});
				}
				else
				{
					winApiService.MoveWindow(currentWindow, rectangle);

					if (!rectangle.Dpi.Equals(currentWindow?.Dpi))
					{
						winApiService.MoveWindow(currentWindow, rectangle);
					}
				}
			}
		}
	}
}