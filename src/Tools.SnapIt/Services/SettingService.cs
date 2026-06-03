using System.Runtime.InteropServices;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Services.Abstractions;
using WindowsDisplayAPI;

namespace Tools.SnapIt.Services;

public class SettingService : ISettingService
{
	private readonly IFileOperationService fileOperationService;

	public bool IsInitialized { get; private set; }
	public Settings Settings { get; private set; }
	public ExcludedApplicationSettings ExcludedApplicationSettings { get; private set; }
	public IList<Layout> Layouts { get; private set; }
	public IList<SnapScreen> SnapScreens { get; private set; }
	public SnapScreen LatestActiveScreen { get; set; }
	public SnapScreen SelectedSnapScreen { get; set; }

	public SettingService(
		IFileOperationService fileOperationService)
	{
		this.fileOperationService = fileOperationService;
	}

	public async Task InitializeAsync()
	{
		if (IsInitialized)
		{
			return;
		}

		await fileOperationService.InitializeAsync();

		await LoadSettingsAsync();

		ExcludedApplicationSettings = await fileOperationService.LoadAsync<ExcludedApplicationSettings>();
		ExcludedApplicationSettings.Applications = ExcludedApplicationSettings.Applications.Where(i => i != null).ToList();

		if (!ExcludedApplicationSettings.Applications.Any(e => e.Keyword == "Program Manager"))
		{
			ExcludedApplicationSettings.Applications.Add(new ExcludedApplication
			{
				Keyword = "Program Manager",
				MatchRule = MatchRule.Contains,
				Mouse = true
			});
		}

		Layouts = fileOperationService.GetLayouts();

		ReInitialize();

		IsInitialized = true;
	}

	public async Task LoadSettingsAsync()
	{
		Settings ??= await fileOperationService.LoadAsync<Settings>() ?? new Settings();
	}

	public void ReInitialize()
	{
		SnapScreens = GetSnapScreens();

		if (LatestActiveScreen == null || SelectedSnapScreen == null)
		{
			LatestActiveScreen = SelectedSnapScreen = SnapScreens.FirstOrDefault(screen => screen.IsPrimary);
		}
	}

	public void LinkScreenLayout(SnapScreen snapScreen, Layout layout)
	{
		var screen = SnapScreens.FirstOrDefault(screen => screen.DeviceName == snapScreen.DeviceName);
		if (screen != null)
		{
			screen.Layout = layout;
		}

		if (Settings.ScreensLayouts.ContainsKey(snapScreen.DeviceName))
		{
			Settings.ScreensLayouts[snapScreen.DeviceName] = layout.Guid.ToString();
		}
		else
		{
			Settings.ScreensLayouts.Add(snapScreen.DeviceName, layout.Guid.ToString());
		}
	}

	public void Dispose()
	{
		IsInitialized = false;
	}

	private IList<SnapScreen> GetSnapScreens()
	{
		var snapScreens = new List<SnapScreen>();

		var displays = Display.GetDisplays();

		foreach (var display in displays)
		{
			var monitorInfo = GetMonitorInfo(display);
			var workingArea = new Rectangle(
				monitorInfo.workArea.left,
				monitorInfo.workArea.top,
				monitorInfo.workArea.right,
				monitorInfo.workArea.bottom);
			var bounds = new Rectangle(
				monitorInfo.monitor.left,
				monitorInfo.monitor.top,
				monitorInfo.monitor.right,
				monitorInfo.monitor.bottom);

			var dpi = Extensions.DpiHelper.GetDpiFromPoint((float)bounds.Left, (float)bounds.Top);
			var scaleFactor = 1.0 / dpi.X;

			var snapScreen = new SnapScreen(display, workingArea, bounds, scaleFactor, monitorInfo.isPrimary);
			var layoutGuid = Settings.ScreensLayouts.TryGetValue(snapScreen.DeviceName, out var value)
				? value : string.Empty;

			if (!string.IsNullOrWhiteSpace(layoutGuid))
			{
				snapScreen.Layout = Layouts.FirstOrDefault(layout => layout.Guid.ToString() == layoutGuid);
			}
			else
			{
				snapScreen.Layout = Layouts.FirstOrDefault();
			}

			snapScreen.IsActive = !Settings.DeactivedScreens.Contains(snapScreen.DeviceName);

			snapScreens.Add(snapScreen);
		}

		return snapScreens;
	}

	[DllImport("user32.dll")]
	private static extern bool GetMonitorInfo(nint hMonitor, ref MONITORINFO lpmi);

	[DllImport("user32.dll")]
	private static extern nint MonitorFromPoint(POINT pt, uint dwFlags);

	[StructLayout(LayoutKind.Sequential)]
	private struct RECT
	{
		public int left, top, right, bottom;
	}

	[StructLayout(LayoutKind.Sequential)]
	private struct MONITORINFO
	{
		public int cbSize;
		public RECT monitor;
		public RECT workArea;
		public uint dwFlags;
	}

	[StructLayout(LayoutKind.Sequential)]
	private struct POINT
	{
		public int x, y;
	}

	private static (RECT monitor, RECT workArea, bool isPrimary) GetMonitorInfo(Display display)
	{
		var mi = new MONITORINFO { cbSize = Marshal.SizeOf<MONITORINFO>() };
		var pt = new POINT { x = display.CurrentSetting.Position.X + 1, y = display.CurrentSetting.Position.Y + 1 };
		var hMonitor = MonitorFromPoint(pt, 2);
		GetMonitorInfo(hMonitor, ref mi);
		return (mi.monitor, mi.workArea, (mi.dwFlags & 1) != 0);
	}
}
