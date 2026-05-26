using Tools.SnapIt.Extensions;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Mvvm;

namespace Tools.SnapIt.Entities;

public class SnapScreen : Bindable
{
	private Layout layout;
	private bool isActive = true;
	private string primary;
	private bool isPrimary = false;
	private string deviceNumber;
	private string resolution;

	public string DeviceName { get; set; }
	public Rectangle WorkingArea { get; set; }
	public double ScaleFactor { get; set; }
	public Rectangle Bounds { get; set; }

	public bool IsActive
	{ get => isActive; set { SetProperty(ref isActive, value); } }

	public bool IsPrimary
	{ get => isPrimary; set { SetProperty(ref isPrimary, value); } }

	public string Primary
	{ get => primary; set { SetProperty(ref primary, value); } }

	public string DeviceNumber
	{ get => deviceNumber; set { SetProperty(ref deviceNumber, value); } }

	public string Resolution
	{ get => resolution; set { SetProperty(ref resolution, value); } }

	public Layout Layout
	{ get => layout; set { SetProperty(ref layout, value); } }

	public SnapScreen()
	{ }

	public SnapScreen(WpfScreenHelper.Screen screen, string devicePath)
	{
		IsPrimary = screen.Primary;
		Primary = IsPrimary ? "Primary" : "";
		DeviceNumber = screen.DeviceName.Replace(@"\\.\DISPLAY", string.Empty);
		Resolution = $"{screen.Bounds.Width} X {screen.Bounds.Height}";

		WorkingArea = screen.WorkingArea.Convert();
		ScaleFactor = screen.ScaleFactor;

		Bounds = screen.WpfBounds.Convert();
		if (!string.IsNullOrEmpty(devicePath))
		{
			DeviceName = devicePath;
		}
		else
		{
			DeviceName = screen.DeviceName;
		}
	}
}