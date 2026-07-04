using System.Collections.Concurrent;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Services;

public class WindowEventService : IWindowEventService
{
    private readonly ISettingService settingService;
    private readonly IWinApiService winApiService;
    private readonly IWindowsService windowsService;
    private nint hookHandle;
    private WinApiService.WinEventDelegate hookDelegate;
    private readonly ConcurrentDictionary<nint, bool> processedWindows = new();
    private readonly object lockObject = new();
    // Cancels the delayed processedWindows eviction tasks so they don't outlive
    // monitoring / the service. Created on StartMonitoring, cancelled on Stop.
    private CancellationTokenSource? _evictionCts;

    [ThreadStatic]
    private static char[] titleBuffer;

    private static char[] GetTitleBuffer()
    {
        titleBuffer ??= new char[257];
        return titleBuffer;
    }

    public bool IsInitialized { get; private set; }
    public bool IsMonitoring { get; private set; }

    public WindowEventService(
        ISettingService settingService,
        IWinApiService winApiService,
        IWindowsService windowsService)
    {
        this.settingService = settingService;
        this.winApiService = winApiService;
        this.windowsService = windowsService;
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

        IsInitialized = true;
    }

    public void StartMonitoring()
    {
        if (!IsInitialized || IsMonitoring)
        {
            return;
        }

        if (!settingService.Settings.EnableAutomaticWindowCornering)
        {
            return;
        }

        hookDelegate = WinEventProc;

        hookHandle = WinApiService.SetWinEventHook(
            WinApiService.EVENT_OBJECT_SHOW,
            WinApiService.EVENT_OBJECT_SHOW,
            nint.Zero,
            hookDelegate,
            0,
            0,
            WinApiService.WINEVENT_OUTOFCONTEXT | WinApiService.WINEVENT_SKIPOWNPROCESS);

        if (hookHandle != nint.Zero)
        {
            _evictionCts ??= new CancellationTokenSource();
            IsMonitoring = true;
            Dev.Log("Window event monitoring started");
        }
        else
        {
            Dev.Log("Failed to start window event monitoring");
        }
    }

    public void StopMonitoring()
    {
        if (!IsMonitoring)
        {
            return;
        }

        if (hookHandle != nint.Zero)
        {
            WinApiService.UnhookWinEvent(hookHandle);
            hookHandle = nint.Zero;
            IsMonitoring = false;
            Dev.Log("Window event monitoring stopped");
        }

        // Cancel any in-flight eviction delays so their tasks don't linger for
        // 30s after monitoring stops, then clear the tracked set.
        _evictionCts?.Cancel();
        _evictionCts?.Dispose();
        _evictionCts = null;

        processedWindows.Clear();
    }

    private void WinEventProc(nint hWinEventHook, uint eventType, nint hwnd, int idObject, int idChild, uint dwEventThread, uint dwmsEventTime)
    {
        if (idObject != WinApiService.OBJID_WINDOW || hwnd == nint.Zero)
        {
            return;
        }

        if (eventType == WinApiService.EVENT_OBJECT_SHOW)
        {
            // Offload the 100ms-delayed processing off the WinEvent callback (it
            // must return fast). Observe faults so exceptions surface in the log
            // instead of dying as an unobserved task exception. _evictionCts is
            // created in StartMonitoring, so it's non-null while monitoring.
            var token = _evictionCts!.Token;
            Task.Run(() => ProcessNewWindow(hwnd, token), token).ContinueWith(
                t => Dev.Log($"Error processing new window: {t.Exception?.GetBaseException()?.Message}"),
                TaskContinuationOptions.OnlyOnFaulted);
        }
    }

    private async Task ProcessNewWindow(nint hwnd, CancellationToken evictionToken)
    {
        try
        {
            if (!processedWindows.TryAdd(hwnd, true))
            {
                return;
            }

            await Task.Delay(100, evictionToken);

            if (!PInvoke.User32.IsWindowVisible(hwnd))
            {
                return;
            }

            var buff = GetTitleBuffer();
            var length = PInvoke.User32.GetWindowText(hwnd, buff, 256);

            if (length == 0)
            {
                return;
            }

            var windowTitle = new string(buff, 0, length);

            if (string.IsNullOrWhiteSpace(windowTitle) || windowTitle.Equals("Program Manager"))
            {
                return;
            }

            if (windowsService.IsExcludedApplication(windowTitle))
            {
                Dev.Log($"Window excluded from cornering: {windowTitle}");
                return;
            }

            var activeWindow = new ActiveWindow
            {
                Handle = hwnd,
                Title = windowTitle
            };

            if (PInvoke.User32.GetWindowRect(hwnd, out PInvoke.RECT rct))
            {
                activeWindow.Boundry = new Rectangle(rct.left, rct.top, rct.right, rct.bottom);
            }

            if (activeWindow.Boundry.IsEmpty)
            {
                return;
            }

            winApiService.SetWindowCornerPreference(activeWindow, DWM_WINDOW_CORNER_PREFERENCE.DWMWCP_DONOTROUND);
        }
        catch (OperationCanceledException)
        {
            // Monitoring stopped mid-processing; expected during teardown.
        }
        catch (Exception ex)
        {
            Dev.Log($"Error processing new window: {ex.Message}");
        }
        finally
        {
            // Auto-evict after 30s so the dictionary stays bounded across long
            // sessions with window churn. Tied to the eviction token so stopping
            // monitoring cancels the wait instead of orphaning the task.
            if (!evictionToken.IsCancellationRequested)
            {
                _ = Task.Run(async () =>
                {
                    try
                    {
                        await Task.Delay(30000, evictionToken);
                        processedWindows.TryRemove(hwnd, out _);
                    }
                    catch (OperationCanceledException)
                    {
                        // Monitoring stopped; nothing to evict.
                    }
                }, evictionToken);
            }
        }
    }

    public void Dispose()
    {
        StopMonitoring();
        IsInitialized = false;
    }
}