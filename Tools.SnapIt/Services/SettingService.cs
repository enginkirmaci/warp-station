using Tools.SnapIt.Entities;
using Tools.SnapIt.Services.Abstractions;
using WpfScreenHelper;

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

		var displays = WindowsDisplayAPI.Display.GetDisplays();

		foreach (var screen in Screen.AllScreens)
		{
			var display = displays.FirstOrDefault(display => display.DisplayName == screen.DeviceName);
			var snapScreen = new SnapScreen(screen, display?.DevicePath);
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
}