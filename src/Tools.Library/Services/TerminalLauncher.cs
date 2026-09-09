using Serilog;
using Tools.Library.Formatters;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Default <see cref="ITerminalLauncher"/>. Centralizes the launch decisions that used to
/// live in the ViewModels: executable resolution (<see cref="ExecutableDefaults"/>),
/// terminal-specific argument shapes (<see cref="TerminalArgumentFormatter"/>), the
/// IDE-vs-shell and AppImage-vs-terminal choices, and the DevTools pipe launch with its
/// direct-spawn fallback.
/// </summary>
public class TerminalLauncher : ITerminalLauncher
{
    private readonly IProcessLauncher _processLauncher;
    private readonly IDevToolsClient _devToolsClient;

    public TerminalLauncher(IProcessLauncher processLauncher, IDevToolsClient devToolsClient)
    {
        _processLauncher = processLauncher;
        _devToolsClient = devToolsClient;
    }

    /// <inheritdoc/>
    public void OpenFolder(string? folderPath) => _processLauncher.StartProcess(folderPath);

    /// <inheritdoc/>
    public void OpenSolution(string solutionPath, string? configuredIdeExecutable)
    {
        // Windows keeps the .sln shell association unless an IDE is configured; other
        // platforms open the solution in an auto-detected .NET IDE (e.g. Rider).
        var ide = ExecutableDefaults.ResolveIde(configuredIdeExecutable);
        if (ide is null)
        {
            if (OperatingSystem.IsWindows())
            {
                _processLauncher.StartProcess(solutionPath);
            }
            else
            {
                Log.Logger.Warning("OpenVisualStudio: no .NET IDE found; configure one in Repos settings");
            }
            return;
        }

        _processLauncher.StartProcess(ide, $"\"{solutionPath}\"", stripElectronEnvironment: true);
    }

    /// <inheritdoc/>
    public async Task OpenInVSCodeAsync(string folderPath, string? configuredExecutable, string? profile)
    {
        // Resolve early: on Linux the GUI PATH can miss user-level installs, so the
        // fallback launch below needs the absolute path, not the bare name.
        var exe = ExecutableDefaults.Locate(configuredExecutable)
                  ?? configuredExecutable
                  ?? "code";

        // When a profile is configured, launch VS Code with it (--profile <name>);
        // otherwise open with the default profile (no extra arguments).
        var args = string.IsNullOrWhiteSpace(profile)
            ? folderPath
            : $"--profile \"{profile}\" \"{folderPath}\"";

        // Route through the DevTools service (named pipe). The service runs
        // non-elevated, so VS Code launches non-elevated even when Tools runs as admin.
        // A .cmd/.bat shim (what a resolved "code" is on Windows) would otherwise show
        // a cmd console when the service launches it via ShellExecute; hidden suppresses
        // just that console. A real GUI exe keeps Normal — the SW_HIDE behind
        // WindowStyle=Hidden must never reach VS Code's own main window.
        var hidden = exe.EndsWith(".cmd", StringComparison.OrdinalIgnoreCase)
                     || exe.EndsWith(".bat", StringComparison.OrdinalIgnoreCase);
        try
        {
            await _devToolsClient.SendProcessLaunchRequestAsync(exe, args, hidden);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenWithVSCode: pipe launch failed, falling back to direct launch");
            _processLauncher.StartProcess(exe, args, hidden: true, stripElectronEnvironment: true);
        }
    }

    /// <inheritdoc/>
    public void OpenFolderInTerminal(string folderPath, string? configuredTerminalExecutable)
    {
        var exe = ExecutableDefaults.ResolveTerminal(configuredTerminalExecutable);
        if (exe is null) return;

        var args = TerminalArgumentFormatter.BuildArguments(exe, folderPath);
        _processLauncher.StartProcess(exe, args, stripElectronEnvironment: true);
    }

    /// <inheritdoc/>
    public void OpenZCode(string folderPath, string? configuredZCodeExecutable, string? configuredTerminalExecutable)
    {
        var resolved = ExecutableDefaults.Locate(configuredZCodeExecutable)
                       ?? configuredZCodeExecutable
                       ?? "zcode";

        // The zcode AppImage is the Electron desktop package (it contains no interactive
        // CLI runtime), so it is launched directly on the folder like VS Code — no
        // terminal wrapper. A standalone zcode CLI binary has no UI of its own, so that
        // variant still runs inside the configured terminal.
        if (!OperatingSystem.IsWindows() && resolved.EndsWith(".appimage", StringComparison.OrdinalIgnoreCase))
        {
            _processLauncher.StartProcess(resolved, $"\"{folderPath}\"", stripElectronEnvironment: true);
            return;
        }

        var terminalExe = ExecutableDefaults.ResolveTerminal(configuredTerminalExecutable);
        if (terminalExe is null) return;

        var zcodeExe = resolved.Contains(' ') ? $"\"{resolved}\"" : resolved;
        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, folderPath, zcodeExe);
        _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
    }

    /// <inheritdoc/>
    public void LaunchOpenCode(string terminalExe, string openCodeExe, string folderPath, string model, string prompt, int count)
    {
        if (string.IsNullOrWhiteSpace(terminalExe) || string.IsNullOrWhiteSpace(folderPath))
            return;

        var commandLine = BuildOpenCodeCommandLine(openCodeExe, model, prompt);
        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, folderPath, commandLine);
        for (var i = 0; i < Math.Max(1, count); i++)
        {
            _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
        }
    }

    /// <summary>
    /// Builds the opencode command line (e.g.
    /// <c>opencode --model "gpt-4" --prompt "fix the bug"</c>) shared by the non-tiled and
    /// grid launch paths so both stay in sync. <c>--model</c>/<c>--prompt</c> are only
    /// emitted when non-empty; values are trimmed and quotes escaped.
    /// </summary>
    public static string BuildOpenCodeCommandLine(string openCodeExe, string model, string prompt)
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
}
