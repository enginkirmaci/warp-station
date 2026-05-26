namespace Tools.SnapIt.Entities;

public class Settings
{
	public Settings()
	{
		ScreensLayouts = [];
		DeactivedScreens = [];
	}

	public string Version = "2.0";
	public bool EnableMouse { get; set; } = true;
	public bool DragByTitle { get; set; } = true;
	public MouseButton MouseButton { get; set; } = MouseButton.Left;
	public int MouseDragDelay { get; set; } = 20;
	public bool EnableHoldKey { get; set; } = false;
	public HoldKey HoldKey { get; set; } = HoldKey.Control;
	public HoldKeyBehaviour HoldKeyBehaviour { get; set; } = HoldKeyBehaviour.HoldToEnable;
	public bool DisableForFullscreen { get; set; } = true;
	public bool DisableForModal { get; set; } = true;
	public bool EnableAutomaticWindowCornering { get; set; } = false;
	public Dictionary<string, string> ScreensLayouts { get; set; }
	public List<string> DeactivedScreens { get; set; }
	public bool RunAsAdmin { get; set; } = true;
	public bool MouseHoverAnimation { get; set; } = false;
	public SnapAreaTheme Theme { get; set; } = new SnapAreaTheme();
}