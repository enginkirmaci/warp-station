using Serilog;
using Tools.Library.Formatters;
using Tools.Library.Services.Abstractions;
using Tools.Services.Abstractions;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.Services;

/// <summary>
/// Default <see cref="IOpenCodeGridLauncher"/>. For each instance it: snapshots the
/// currently open top-level windows, launches a terminal running opencode, polls
/// <see cref="IWinApiService.GetOpenWindows"/> for the newly-appeared handle, then moves
/// that window into a grid cell derived from the active screen's working area. The grid
/// dimensions auto-compute from the instance count (6 -> 3 columns x 2 rows).
/// </summary>
public class OpenCodeGridLauncher : IOpenCodeGridLauncher
{
    private const string OpenCodeWindowTitleHint = "opencode";

    // How long to wait for a launched window to show up before giving up on tiling it.
    private static readonly TimeSpan WindowAppearTimeout = TimeSpan.FromSeconds(10);
    // Interval between polls for the new window handle.
    private static readonly TimeSpan PollInterval = TimeSpan.FromMilliseconds(150);

    private readonly IProcessLauncher _processLauncher;
    private readonly IWinApiService _winApiService;
    private readonly ISettingService _settingService;

    public OpenCodeGridLauncher(
        IProcessLauncher processLauncher,
        IWinApiService winApiService,
        ISettingService settingService)
    {
        _processLauncher = processLauncher;
        _winApiService = winApiService;
        _settingService = settingService;
    }

    public async Task LaunchAsync(
        string terminalExe,
        string openCodeExe,
        string folderPath,
        string model,
        string prompt,
        int count)
    {
        if (string.IsNullOrWhiteSpace(terminalExe) || string.IsNullOrWhiteSpace(folderPath))
            return;

        count = Math.Max(1, count);
        var (cols, rows) = ComputeGrid(count);
        var cells = BuildCells(ResolveWorkingArea(), cols, rows, count);

        var commandLine = BuildCommandLine(openCodeExe, model, prompt);
        // Force each instance into its own window so they can be tiled individually
        // (otherwise Windows Terminal merges them into tabs of one window).
        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, folderPath, commandLine, forceNewWindow: true);

        for (var i = 0; i < count; i++)
        {
            var beforeHandles = _winApiService.GetOpenWindows().Keys.ToHashSet();

            _processLauncher.StartProcess(terminalExe, args);

            var handle = await WaitForNewWindowAsync(beforeHandles, OpenCodeWindowTitleHint);
            if (handle == nint.Zero)
            {
                Log.Logger.Warning(
                    "OpenCodeGridLauncher: timed out waiting for instance {Index}/{Count}; skipping positioning",
                    i + 1, count);
                continue;
            }

            if (i < cells.Count)
            {
                MoveIntoCell(handle, cells[i]);
            }
        }
    }

    // --- helpers ---

    private static string BuildCommandLine(string openCodeExe, string model, string prompt)
    {
        openCodeExe = string.IsNullOrWhiteSpace(openCodeExe) ? "opencode" : openCodeExe;
        var cleanModel = (model ?? string.Empty).Trim();
        var cleanPrompt = (prompt ?? string.Empty).Trim();

        var parts = new List<string> { openCodeExe };
        if (!string.IsNullOrWhiteSpace(cleanModel))
        {
            parts.Add($"--model \"{Escape(cleanModel)}\"");
        }
        if (!string.IsNullOrWhiteSpace(cleanPrompt))
        {
            parts.Add($"--prompt \"{Escape(cleanPrompt)}\"");
        }

        return string.Join(' ', parts);
    }

    private static string Escape(string value) => value.Replace("\"", "\\\"");

    /// <summary>
    /// Picks the most square-ish grid that fits the count (6 -> 3x2, 4 -> 2x2, 8 -> 3x3).
    /// Columns are rounded up so the last row may be partial.
    /// </summary>
    internal static (int cols, int rows) ComputeGrid(int count)
    {
        count = Math.Max(1, count);
        var cols = (int)Math.Ceiling(Math.Sqrt(count));
        var rows = (int)Math.Ceiling(count / (double)cols);
        return (cols, rows);
    }

    /// <summary>
    /// Computes the absolute screen rectangles for the first <paramref name="count"/> cells
    /// of a <paramref name="cols"/> x <paramref name="rows"/> grid laid over
    /// <paramref name="area"/>, filling row by row. Returns only as many cells as requested.
    /// </summary>
    internal static List<Rectangle> BuildCells(Rectangle area, int cols, int rows, int count)
    {
        var cells = new List<Rectangle>(count);
        if (area.IsEmpty || cols < 1 || rows < 1) return cells;

        var cellWidth = area.Width / cols;
        var cellHeight = area.Height / rows;

        for (var i = 0; i < count; i++)
        {
            var col = i % cols;
            var row = i / cols;
            var left = area.Left + col * cellWidth;
            var top = area.Top + row * cellHeight;
            // The last cell in each dimension extends to the edge to absorb rounding error
            // so there are no gaps at the right/bottom of the screen.
            var width = (col == cols - 1) ? area.Right - left : cellWidth;
            var height = (row == rows - 1) ? area.Bottom - top : cellHeight;
            cells.Add(new Rectangle(left, top, left + width, top + height));
        }

        return cells;
    }

    private Rectangle ResolveWorkingArea()
    {
        var screen = _settingService.SelectedSnapScreen
                     ?? _settingService.LatestActiveScreen
                     ?? _settingService.SnapScreens?.FirstOrDefault(s => s.IsPrimary)
                     ?? _settingService.SnapScreens?.FirstOrDefault();

        return screen?.WorkingArea ?? Rectangle.Empty;
    }

    /// <summary>
    /// Polls <see cref="IWinApiService.GetOpenWindows"/> for a handle that was not present
    /// before launch. When <paramref name="titleHint"/> is provided, prefers a window whose
    /// title contains that hint (the opencode TUI sets its title); otherwise returns the
    /// first new handle. Returns <see cref="nint.Zero"/> on timeout.
    /// </summary>
    private async Task<nint> WaitForNewWindowAsync(HashSet<nint> beforeHandles, string? titleHint)
    {
        var deadline = DateTime.UtcNow + WindowAppearTimeout;
        nint fallbackNewHandle = nint.Zero;

        while (DateTime.UtcNow < deadline)
        {
            await Task.Delay(PollInterval);

            var current = _winApiService.GetOpenWindows();
            foreach (var (handle, title) in current)
            {
                if (beforeHandles.Contains(handle)) continue;

                // Prefer the opencode window when its title has been set.
                if (!string.IsNullOrEmpty(titleHint)
                    && title != null
                    && title.Contains(titleHint, StringComparison.OrdinalIgnoreCase))
                {
                    return handle;
                }

                fallbackNewHandle = handle;
            }

            if (fallbackNewHandle != nint.Zero)
            {
                // A new window appeared; once is enough. Keep looping briefly only if we're
                // still hunting for the hinted title, otherwise return immediately.
                if (string.IsNullOrEmpty(titleHint)) return fallbackNewHandle;
            }
        }

        return fallbackNewHandle;
    }

    /// <summary>
    /// Moves the window into <paramref name="cell"/>, correcting for the invisible DWM
    /// extended-frame margins the same way the interactive SnapIt path does
    /// (<c>SnapManager.MoveWindow</c>) so the visible client fills the cell cleanly.
    /// </summary>
    private void MoveIntoCell(nint handle, Rectangle cell)
    {
        if (handle == nint.Zero || cell.IsEmpty) return;

        var activeWindow = new ActiveWindow { Handle = handle };

        _winApiService.GetWindowMargin(activeWindow, out var withMargin);
        var outer = _winApiService.GetWindowRect(handle);
        if (!withMargin.IsEmpty && !outer.IsEmpty)
        {
            var marginHorizontal = (outer.Width - withMargin.Width) / 2;
            cell.Left -= marginHorizontal;
            cell.Top -= 0;
            cell.Right += marginHorizontal;
            cell.Bottom += outer.Height - withMargin.Height;
        }

        _winApiService.MoveWindow(activeWindow, cell);

        // The opencode TUI renders asynchronously; re-apply once shortly after to make sure
        // the position sticks once its window finishes initializing.
        _ = Task.Run(async () =>
        {
            await Task.Delay(250);
            _winApiService.MoveWindow(activeWindow, cell);
        });
    }
}
