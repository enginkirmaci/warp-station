using System.Runtime.InteropServices;
using Avalonia;
using Avalonia.Threading;
using SharpHook;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Events;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Services.Abstractions;
using MouseButton = Tools.SnapIt.Entities.MouseButton;

namespace Tools.SnapIt.Services;

public class MouseService : IMouseService
{
	private readonly ISettingService settingService;
	private readonly IWinApiService winApiService;
	private readonly IWindowsService windowsService;
	private readonly IGlobalHookService globalHookService;
	private volatile bool isWindowDetected;
	private volatile bool isListening;
	private volatile bool isHoldingKey;
	private volatile bool holdKeyUsed;
	private ActiveWindow activeWindow;
	private SnapAreaInfo? snapAreaInfo;
	private System.Drawing.Point startLocation;

	// Latest mouse position reported by the global hook. The hook thread only
	// writes this + schedules a single UI-thread drain; it never blocks on the
	// UI thread. Coalescing means we process the most recent position under
	// load and drop intermediate moves, instead of rendezvousing with the UI
	// thread on every MouseDragged event (which stalled the hook).
	private Point _latestMousePos;
	private volatile bool _uiBusy;

	public bool IsInitialized { get; private set; }

	public event HideWindowsEvent HideWindows;

	public event MoveWindowEvent MoveWindow;

	public event SnappingCancelEvent SnappingCancelled;

	public event ShowWindowsIfNecessaryEvent ShowWindowsIfNecessary;

	public event SelectElementWithPointEvent SelectElementWithPoint;

	public MouseService(
		ISettingService settingService,
		IWinApiService winApiService,
		IWindowsService windowsService,
		IGlobalHookService globalHookService)
	{
		this.settingService = settingService;
		this.winApiService = winApiService;
		this.windowsService = windowsService;
		this.globalHookService = globalHookService;
	}

	public async Task InitializeAsync()
	{
		if (IsInitialized)
		{
			return;
		}

		await settingService.InitializeAsync();
		await winApiService.InitializeAsync();
		await windowsService.InitializeAsync();
		await globalHookService.InitializeAsync();

		if (globalHookService.Hook != null && settingService.Settings.EnableMouse)
		{
			globalHookService.Hook.MouseDragged += MouseMoveEvent;
			globalHookService.Hook.MousePressed += MouseDownEvent;
			globalHookService.Hook.MouseReleased += MouseUpEvent;
			globalHookService.Hook.KeyPressed += Esc_KeyDown;

			if (settingService.Settings.EnableHoldKey)
			{
				globalHookService.Hook.KeyPressed += KeyDown;
				globalHookService.Hook.KeyReleased += KeyUp;
			}
		}

		isWindowDetected = false;
		isListening = false;

		IsInitialized = true;
	}

	public void Dispose()
	{
		if (globalHookService.Hook != null)
		{
			globalHookService.Hook.MouseDragged -= MouseMoveEvent;
			globalHookService.Hook.MousePressed -= MouseDownEvent;
			globalHookService.Hook.MouseReleased -= MouseUpEvent;
			globalHookService.Hook.KeyPressed -= Esc_KeyDown;

			globalHookService.Hook.KeyPressed -= KeyDown;
			globalHookService.Hook.KeyReleased -= KeyUp;
		}

		IsInitialized = false;
	}

	public void Interrupt()
	{
		isListening = false;
	}

	private void MouseMoveEvent(object? sender, MouseHookEventArgs e)
	{
		if (!isListening)
		{
			return;
		}

		// Hook thread: stash the latest position and ensure exactly one UI-thread
		// drain is in flight. Never block here — the old code did
		// Dispatcher.UIThread.InvokeAsync(...).GetAwaiter().GetResult() on every
		// move, deadlocking the hook against the UI thread.
		_latestMousePos = new Point(e.Data.X, e.Data.Y);
		if (_uiBusy)
		{
			return;
		}

		_uiBusy = true;
		Dispatcher.UIThread.Post(ProcessMoveOnUIThread);
	}

	private void ProcessMoveOnUIThread()
	{
		try
		{
			// Drain in a loop so we always act on the most recent position seen
			// so far. If a newer move arrived while we were processing, loop again
			// instead of scheduling another Post.
			Point p;
			do
			{
				p = _latestMousePos;
				ProcessSingleMove(p);
			}
			while (p != _latestMousePos);
		}
		finally
		{
			_uiBusy = false;
		}
	}

	private void ProcessSingleMove(Point p)
	{
		if (!isListening)
		{
			return;
		}

		if (HoldingKeyResult() && IsDelayDone(p))
		{
			if (!isWindowDetected)
			{
				holdKeyUsed = true;

				activeWindow = winApiService.GetActiveWindow();
				activeWindow.Dpi = DpiHelper.GetDpiFromPoint((int)p.X, (int)p.Y);

				if (activeWindow?.Title != null && windowsService.IsExcludedApplication(activeWindow.Title))
				{
					isListening = false;
				}
				else if (settingService.Settings.DisableForFullscreen && winApiService.IsFullscreen(activeWindow))
				{
					isListening = false;
				}
				else if (settingService.Settings.DisableForModal && !winApiService.IsAllowedWindowStyle(activeWindow))
				{
					isListening = false;
				}
				else if (settingService.Settings.DragByTitle)
				{
					var titleBarHeight = GetSystemMetrics(SM_CYCAPTION);
					var FixedFrameBorderSize = GetSystemMetrics(SM_CYFIXEDFRAME);

					if (activeWindow.Boundry.Top + titleBarHeight + 2 + FixedFrameBorderSize * 2 >= p.Y)
					{
						isWindowDetected = true;
					}
					else
					{
						isListening = false;
					}
				}
				else
				{
					isWindowDetected = true;
				}
			}
			else if (ShowWindowsIfNecessary != null && ShowWindowsIfNecessary.Invoke())
			{
			}
			else
			{
				snapAreaInfo = SelectElementWithPoint?.Invoke((int)p.X, (int)p.Y);

				if (snapAreaInfo?.Screen != null)
				{
					settingService.LatestActiveScreen = snapAreaInfo.Screen;
				}
			}
		}
	}

	private void MouseDownEvent(object? sender, MouseHookEventArgs e)
	{
		if (e.Data.Button == MouseButtonsMap(settingService.Settings.MouseButton))
		{
			globalHookService.Hook.MouseDragged += MouseMoveEvent;

			activeWindow = ActiveWindow.Empty;
			snapAreaInfo = SnapAreaInfo.Empty;
			isWindowDetected = false;
			isListening = true;

			startLocation = new System.Drawing.Point(e.Data.X, e.Data.Y);
		}
	}

	private void MouseUpEvent(object? sender, MouseHookEventArgs e)
	{
		if (e.Data.Button == MouseButtonsMap(settingService.Settings.MouseButton) && isListening)
		{
			globalHookService.Hook.MouseDragged -= MouseMoveEvent;

			isListening = false;
			HideWindows?.Invoke();

			MoveWindow?.Invoke(new SnapAreaInfo
			{
				ActiveWindow = activeWindow,
				Rectangle = snapAreaInfo?.Rectangle
			}, e.Data.Button == SharpHook.Data.MouseButton.Button1);
		}
	}

	private void Esc_KeyDown(object? sender, KeyboardHookEventArgs e)
	{
		if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcEscape)
		{
			SnappingCancelled?.Invoke();
		}
	}

	private void KeyUp(object? sender, KeyboardHookEventArgs e)
	{
		switch (settingService.Settings.HoldKey)
		{
			case HoldKey.Control:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftControl || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightControl)
				{
					isHoldingKey = false;
				}

				break;

			case HoldKey.Alt:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftAlt || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightAlt)
				{
					isHoldingKey = false;
				}

				break;

			case HoldKey.Shift:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftShift || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightShift)
				{
					isHoldingKey = false;
				}

				break;

			case HoldKey.Win:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftMeta || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightMeta)
				{
					isHoldingKey = false;

					if (holdKeyUsed)
					{
						e.SuppressEvent = true;
					}
				}

				break;
		}

		if (holdKeyUsed)
		{
			holdKeyUsed = false;
		}
	}

	private void KeyDown(object? sender, KeyboardHookEventArgs e)
	{
		switch (settingService.Settings.HoldKey)
		{
			case HoldKey.Control:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftControl || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightControl)
				{
					isHoldingKey = true;
				}

				break;

			case HoldKey.Alt:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftAlt || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightAlt)
				{
					isHoldingKey = true;
				}

				break;

			case HoldKey.Shift:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftShift || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightShift)
				{
					isHoldingKey = true;
				}

				break;

			case HoldKey.Win:
				if (e.Data.KeyCode == SharpHook.Data.KeyCode.VcLeftMeta || e.Data.KeyCode == SharpHook.Data.KeyCode.VcRightMeta)
				{
					isHoldingKey = true;
				}

				break;
		}
	}

	private bool IsDelayDone(Point endLocation)
	{
		if (settingService.Settings.EnableHoldKey)
			return true;

		var move = System.Math.Abs(endLocation.X - startLocation.X) + System.Math.Abs(endLocation.Y - startLocation.Y);
		return move > settingService.Settings.MouseDragDelay;
	}

	private SharpHook.Data.MouseButton MouseButtonsMap(MouseButton mouseButton)
	{
		switch (mouseButton)
		{
			case MouseButton.Right:
				return SharpHook.Data.MouseButton.Button2;

			case MouseButton.Middle:
				return SharpHook.Data.MouseButton.Button3;

			case MouseButton.XButton1:
				return SharpHook.Data.MouseButton.Button4;

			case MouseButton.XButton2:
				return SharpHook.Data.MouseButton.Button5;

			case MouseButton.Left:
			default:
				return SharpHook.Data.MouseButton.Button1;
		}
	}

	private bool HoldingKeyResult()
	{
		if (settingService.Settings.EnableHoldKey)
		{
			if (isHoldingKey)
			{
				switch (settingService.Settings.HoldKeyBehaviour)
				{
					case HoldKeyBehaviour.HoldToEnable:
						return true;

					case HoldKeyBehaviour.HoldToDisable:
						SnappingCancelled?.Invoke();

						return false;
				}
			}
			else
			{
				switch (settingService.Settings.HoldKeyBehaviour)
				{
					case HoldKeyBehaviour.HoldToEnable:
						return false;

					case HoldKeyBehaviour.HoldToDisable:
						return true;
				}
			}
		}

		return true;
	}

	[DllImport("user32.dll")]
	private static extern int GetSystemMetrics(int nIndex);

	private const int SM_CYCAPTION = 4;
	private const int SM_CYFIXEDFRAME = 8;
}
