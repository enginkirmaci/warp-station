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

	public SnapScreen(WindowsDisplayAPI.Display display, Rectangle workingArea, Rectangle bounds, double scaleFactor, bool isPrimary)
	{
		IsPrimary = isPrimary;
		Primary = IsPrimary ? "Primary" : "";
		DeviceNumber = display.DisplayName.Replace(@"\\.\DISPLAY", string.Empty);
		Resolution = $"{(int)bounds.Width} X {(int)bounds.Height}";

		WorkingArea = workingArea;
		ScaleFactor = scaleFactor;
		Bounds = bounds;
		DeviceName = display.DevicePath ?? display.DisplayName;
	}
}
