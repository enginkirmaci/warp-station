namespace Tools.Library.Formatters;

/// <summary>
/// Builds command-line arguments for launching a terminal at a given folder path,
/// dispatching on the terminal executable (Windows Terminal, PowerShell, cmd, or a
/// generic fallback). Encapsulates terminal-specific argument knowledge so it is not
/// duplicated inside ViewModels.
/// </summary>
public static class TerminalArgumentFormatter
{
    /// <summary>
    /// Builds the launch arguments that open <paramref name="folderPath"/> in the given
    /// terminal executable.
    /// </summary>
    /// <param name="terminalExecutable">The terminal executable path or command.</param>
    /// <param name="folderPath">The folder to open the terminal in.</param>
    /// <returns>The terminal-specific argument string.</returns>
    public static string BuildArguments(string terminalExecutable, string folderPath)
    {
        var exeLower = (terminalExecutable ?? string.Empty).ToLowerInvariant();

        if (exeLower.EndsWith("wt.exe") || exeLower == "wt")
            return $"-d \"{folderPath}\"";

        if (exeLower.Contains("powershell") || exeLower.Contains("pwsh"))
            return $"-NoExit -Command \"Set-Location -LiteralPath '{folderPath}'\"";

        if (exeLower.EndsWith("cmd.exe") || exeLower == "cmd")
            return $"/k cd /d \"{folderPath}\"";

        return $"\"{folderPath}\"";
    }

    /// <summary>
    /// Builds the launch arguments that open <paramref name="folderPath"/> in the given
    /// terminal executable and run <paramref name="commandLine"/> in it. Dispatches on
    /// the terminal executable. Used by the OpenCode flow, where
    /// <paramref name="commandLine"/> is the opencode invocation.
    /// </summary>
    /// <param name="terminalExecutable">The terminal executable path or command.</param>
    /// <param name="folderPath">The folder to open the terminal in.</param>
    /// <param name="commandLine">The command line to run after the terminal opens.</param>
    /// <param name="forceNewWindow">
    /// When <see langword="true"/>, request that the terminal open in a standalone window
    /// rather than reusing an existing instance (e.g. a new tab). Only honored by terminals
    /// that support it (Windows Terminal); ignored otherwise.
    /// </param>
    /// <returns>The terminal-specific argument string that cds then runs the command.</returns>
    public static string BuildCommandArguments(string terminalExecutable, string folderPath, string commandLine, bool forceNewWindow = false)
    {
        var exeLower = (terminalExecutable ?? string.Empty).ToLowerInvariant();

        if (exeLower.EndsWith("wt.exe") || exeLower == "wt")
        {
            // `-w 0 new` targets a brand-new Windows Terminal window instead of a
            // tab in an already-running instance, which is required when tiling
            // several windows into a grid (otherwise they collapse into one window).
            var windowTarget = forceNewWindow ? "-w 0 new " : string.Empty;

            // Run the command through `cmd /k` rather than directly after `--`.
            // Windows Terminal resolves the command after `--` against `.exe`
            // files only and does NOT apply PATHEXT, so npm-installed CLIs that
            // ship as `.cmd` shims (e.g. opencode) are not found (error
            // 0x80070002). `cmd` resolves `.cmd`/`.bat` shims via PATHEXT, and
            // `/k` keeps the window open after the command exits.
            return $"{windowTarget}-d \"{folderPath}\" -- cmd /k {commandLine}";
        }

        if (exeLower.Contains("powershell") || exeLower.Contains("pwsh"))
            return $"-NoExit -Command \"Set-Location -LiteralPath '{folderPath}'; {commandLine}\"";

        if (exeLower.EndsWith("cmd.exe") || exeLower == "cmd")
            return $"/k cd /d \"{folderPath}\" && {commandLine}";

        // Generic fallback: just open the terminal at the folder. We cannot reliably
        // run an arbitrary command for an unknown terminal, so the command is dropped.
        return $"\"{folderPath}\"";
    }
}
