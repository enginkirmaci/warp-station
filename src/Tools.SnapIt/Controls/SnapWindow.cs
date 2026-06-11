using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Platform;
using Avalonia.VisualTree;
using Tools.SnapIt.Contracts;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Controls;

public class SnapWindow : Window, IWindow
{
	private readonly ISettingService settingService;
	private readonly IWinApiService winApiService;
	private SnapArea currentArea;
	private SnapOverlay currentOverlay;

	public SnapScreen Screen { get; set; }
	public List<Rectangle> SnapAreaBoundries { get; set; }
	public Dictionary<int, Rectangle> SnapAreaRectangles { get; set; }
	public Dpi Dpi { get; set; }

	event EventHandler IWindow.Loaded
	{
		add => Loaded += (s, e) => value?.Invoke(s, EventArgs.Empty);
		remove { }
	}

	public SnapWindow(
		ISettingService settingService,
		IWinApiService winApiService,
		SnapScreen screen)
	{
		this.settingService = settingService;
		this.winApiService = winApiService;

		Screen = screen;

		if (!Dev.IsTopmostDisabled)
		{
			Topmost = true;
		}

		TransparencyLevelHint = [WindowTransparencyLevel.Transparent];
		WindowDecorations = Avalonia.Controls.WindowDecorations.None;
		Background = Brushes.Transparent;
		CanResize = false;
		ShowInTaskbar = false;
		Width = screen.WorkingArea.Width;
		Height = screen.WorkingArea.Height;
		Position = new PixelPoint((int)screen.WorkingArea.X, (int)screen.WorkingArea.Y);
		WindowState = WindowState.Normal;

		Dpi = new Dpi()
		{
			X = (float)(100 / (screen.ScaleFactor * 100)),
			Y = (float)(100 / (screen.ScaleFactor * 100))
		};
	}

	public new void Show()
	{
		base.Show();
		MaximizeWindow();
	}

	private void MaximizeWindow()
	{
		var handle = TryGetPlatformHandle()?.Handle ?? nint.Zero;
		if (handle == nint.Zero) return;

		var window = new ActiveWindow
		{
			Handle = handle
		};

		if (Screen != null)
			winApiService.MoveWindow(
				window,
				(int)Screen.WorkingArea.Left,
				(int)Screen.WorkingArea.Top,
				(int)Screen.WorkingArea.Width,
				(int)Screen.WorkingArea.Height);
	}

	public void ApplyLayout()
	{
		var snapControl = new SnapControl
		{
			SnapTheme = settingService.Settings.Theme,
			Layout = Screen.Layout
		};

		Content = snapControl;
	}

	public void GenerateSnapAreaBoundries()
	{
		if (SnapAreaBoundries == null)
		{
			SnapAreaBoundries = [];
			SnapAreaRectangles = [];

			var snapControl = Content as SnapControl;
			var snapAreas = snapControl.FindChildren<SnapArea>();
			var snapOverlays = snapControl.FindChildren<SnapOverlay>();

			var areaIndex = 0;

			foreach (var snapOverlay in snapOverlays)
			{
				SnapAreaRectangles.Add(areaIndex++, snapOverlay.ScreenSnapArea(Dpi));
			}

			foreach (var snapArea in snapAreas)
			{
				var rectangle = snapArea.ScreenSnapArea(Dpi);

				SnapAreaRectangles.Add(areaIndex++, rectangle);
				SnapAreaBoundries.Add(rectangle);
			}

			SnapAreaBoundries = SnapAreaBoundries.OrderBy(i => i.X).ThenBy(i => i.Y).ToList();
		}
	}

	public new void Hide()
	{
		base.Hide();
	}

	public Rectangle SelectElementWithPoint(int x, int y)
	{
		if (IsVisible)
		{
			var Point2Window = this.PointToClient(new PixelPoint(x, y));

			var element = this.GetVisualAt(Point2Window);

			AvaloniaObject dependencyObject = null;
			if (element != null && element is AvaloniaObject)
			{
				dependencyObject = ((AvaloniaObject)element).FindParent<SnapArea>();
				if (dependencyObject == null)
				{
					dependencyObject = ((AvaloniaObject)element).FindParent<SnapOverlay>();
				}
			}

			if (dependencyObject is SnapArea && currentArea?.Name != ((SnapArea)dependencyObject).Name)
			{
				currentArea?.NormalStyle();
				currentOverlay?.NormalStyle();
			}
			else if (dependencyObject is not SnapArea)
			{
				currentArea?.NormalStyle();
			}

			if (dependencyObject is SnapOverlay && currentOverlay?.Name != ((SnapOverlay)dependencyObject).Name)
			{
				currentArea?.NormalStyle();
				currentOverlay?.NormalStyle();
			}
			else if (dependencyObject is not SnapOverlay)
			{
				currentOverlay?.NormalStyle();
			}

			if (dependencyObject != null)
			{
				if (dependencyObject is SnapArea snapArea)
				{
					if (!(currentArea != null && currentArea.Name == snapArea.Name))
					{
						snapArea.OnHoverStyle();
					}

					currentArea = snapArea;
					currentOverlay = null;

					return snapArea.ScreenSnapArea(Dpi);
				}

				if (dependencyObject is SnapOverlay snapOverlay)
				{
					if (!(currentOverlay != null && currentOverlay?.Name == snapOverlay.Name))
					{
						snapOverlay.OnHoverStyle();
					}

					currentArea = null;
					currentOverlay = snapOverlay;

					return snapOverlay.ScreenSnapArea(Dpi);
				}
			}
			else
			{
				currentArea = null;
				currentOverlay = null;
			}
		}

		return Rectangle.Empty;
	}
}
