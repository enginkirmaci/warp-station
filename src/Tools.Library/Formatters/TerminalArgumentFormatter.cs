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
}
