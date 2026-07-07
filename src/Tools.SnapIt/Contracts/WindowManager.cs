using Tools.SnapIt.Controls;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Contracts;

public class WindowManager : IWindowManager
{
	private readonly ISettingService settingService;
	private readonly IWinApiService winApiService;
	private readonly IMouseService mouseService;
	private List<SnapWindow> snapWindows;
	public bool IsInitialized { get; private set; }

	public WindowManager(
		ISettingService settingService,
		IWinApiService winApiService,
		IMouseService mouseService)
	{
		this.settingService = settingService;
		this.winApiService = winApiService;
		this.mouseService = mouseService;
	}

	public async Task InitializeAsync()
	{
		if (IsInitialized)
		{
			return;
		}

		await settingService.InitializeAsync();
		await winApiService.InitializeAsync();
		await mouseService.InitializeAsync();

		mouseService.HideWindows += MouseService_HideWindows;
		mouseService.ShowWindowsIfNecessary += MouseService_ShowWindowsIfNecessary;
		mouseService.SelectElementWithPoint += MouseService_SelectElementWithPoint;

		snapWindows = [];

		foreach (var screen in settingService.SnapScreens)
		{
			if (screen.IsActive)
			{
				var window = new SnapWindow(settingService, winApiService, screen);

				if (!snapWindows.Any(i => i.Screen == screen))
				{
					window.ApplyLayout();

					snapWindows.Add(window);
				}
			}
		}

		snapWindows.ForEach(window =>
		{
			window.Opacity = 0;
			window.Show();
			window.GenerateSnapAreaBoundries();
			window.Hide();
			window.Opacity = 100;
		});

		IsInitialized = true;
	}

	public void Show()
	{
		snapWindows.ForEach(window =>
		{
			window.Show();
			window.Activate();
		});
	}

	public void Hide()
	{
		snapWindows.ForEach(window =>
		{
			window.Hide();
		});
	}

	public void Dispose()
	{
		mouseService.HideWindows -= MouseService_HideWindows;
		mouseService.ShowWindowsIfNecessary -= MouseService_ShowWindowsIfNecessary;
		mouseService.SelectElementWithPoint -= MouseService_SelectElementWithPoint;

		if (snapWindows != null && snapWindows.Count != 0)
		{
			for (int i = 0; i < snapWindows.Count; i++)
			{
				try
				{
					snapWindows[i].Close();
				}
				catch { }
			}

			snapWindows.Clear();
		}

		IsInitialized = false;
	}

	private void MouseService_HideWindows()
	{
		Dispatcher.UIThread.Post(Hide);
	}

	private bool MouseService_ShowWindowsIfNecessary()
	{
		// Invoked on the UI thread by MouseService (see ProcessMoveOnUIThread).
		// No Dispatcher marshalling here — wrapping this in InvokeAsync would
		// deadlock (UI thread synchronously waiting for itself).
		if (!snapWindows.TrueForAll(window => window.IsVisible))
		{
			Show();

			return true;
		}

		return false;
	}

	private SnapAreaInfo MouseService_SelectElementWithPoint(int x, int y)
	{
		// Invoked on the UI thread by MouseService. Runs directly so it never
		// blocks the hook thread (the hook now only posts the latest position).
		var result = new SnapAreaInfo();

		foreach (var window in snapWindows)
		{
			var selectedArea = window.SelectElementWithPoint(x, y);
			if (!selectedArea.Equals(Rectangle.Empty))
			{
				result.Rectangle = selectedArea;
				result.Screen = window.Screen;

				break;
			}
		}

		return result;
	}
}
