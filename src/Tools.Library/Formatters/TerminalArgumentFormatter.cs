namespace Tools.Library.Formatters;

/// <summary>
/// Builds command-line arguments for launching a terminal at a given folder path,
/// dispatching on the terminal executable (Windows Terminal, PowerShell, Linux
/// terminal emulators, or a generic fallback). Encapsulates terminal-specific
/// argument knowledge so it is not duplicated inside ViewModels.
/// <para>
/// The sh and PowerShell paths neutralize shell metacharacters in prompt text
/// (<see cref="ShEscape"/>/<see cref="PsEscape"/>, single-quote escaping for the
/// cd/Set-Location folder). The `cmd /k` branch does not: cmd's own quoting rules
/// make full neutralization impractical here — quoted segments already protect
/// `&amp;|&lt;&gt;^`, but `%VAR%` expansion and unbalanced quotes in prompt text
/// remain known limitations on that branch.
/// </para>
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
            return $"-NoExit -Command \"Set-Location -LiteralPath '{PsEscapeSingleQuoted(folderPath)}'\"";

        if (exeLower.Contains("gnome-terminal") || exeLower == "kgx")
            return $"--working-directory=\"{folderPath}\"";

        if (exeLower.Contains("konsole"))
            return $"--workdir \"{folderPath}\"";

        if (exeLower.Contains("xfce4-terminal"))
            return $"--working-directory=\"{folderPath}\"";

        if (exeLower.Contains("alacritty"))
            return $"--working-directory \"{folderPath}\"";

        if (exeLower.Contains("kitty"))
            return $"--directory \"{folderPath}\"";

        if (exeLower.Contains("wezterm"))
            return $"start --cwd \"{folderPath}\"";

        // xterm/uxterm have no working-directory flag; cd inside a spawned shell instead.
        if (exeLower is "xterm" or "uxterm")
            return $"-e {BuildShellCommand(folderPath, commandLine: null)}";

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
            return $"{windowTarget}-d \"{folderPath}\" -- cmd /k {commandLine.Replace("\"", "\\\"")}";
        }

        if (exeLower.Contains("powershell") || exeLower.Contains("pwsh"))
        {
            var script = $"Set-Location -LiteralPath '{PsEscapeSingleQuoted(folderPath)}'; {EncodePowerShell(commandLine)}";
            // The script travels as ONE argv element: its structural double quotes are
            // backslash-escaped for the argument-string parser, which reconstructs
            // them as real quotes in the -Command text PowerShell parses.
            return $"-NoExit -Command \"{script.Replace("\"", "\\\"")}\"";
        }

        var shellCommand = BuildShellCommand(folderPath, commandLine);

        if (exeLower.Contains("gnome-terminal") || exeLower == "kgx")
            return $"--working-directory=\"{folderPath}\" -- {shellCommand}";

        if (exeLower.Contains("konsole"))
            return $"--workdir \"{folderPath}\" -e {shellCommand}";

        if (exeLower.Contains("xfce4-terminal"))
            return $"--working-directory=\"{folderPath}\" -x {shellCommand}";

        if (exeLower.Contains("alacritty"))
            return $"--working-directory \"{folderPath}\" -e {shellCommand}";

        if (exeLower.Contains("kitty"))
            return $"--directory \"{folderPath}\" {shellCommand}";

        if (exeLower.Contains("wezterm"))
            return $"start --cwd \"{folderPath}\" -- {shellCommand}";

        if (exeLower is "xterm" or "uxterm")
            return $"-e {shellCommand}";

        // Generic fallback: just open the terminal at the folder. We cannot reliably
        // run an arbitrary command for an unknown terminal, so the command is dropped.
        return $"\"{folderPath}\"";
    }

    /// <summary>
    /// Builds a `sh -c` invocation that cds into <paramref name="folderPath"/>, runs
    /// <paramref name="commandLine"/> when given, and then drops into the user's shell —
    /// the Linux counterpart of `cmd /k`, keeping the terminal window open afterwards.
    /// <paramref name="commandLine"/> arrives with value quotes already escaped as
    /// <c>\"</c> (see <c>TerminalLauncher.BuildOpenCodeCommandLine</c>); the sh encoder
    /// below re-encodes those for the argument-string parser and neutralizes `$` and
    /// the backtick so prompt text never expands or executes at launch. The folder is
    /// single-quoted with embedded single quotes escaped.
    /// </summary>
    private static string BuildShellCommand(string folderPath, string? commandLine)
    {
        var cd = $"cd '{folderPath.Replace("'", "'\\''")}'";
        var payload = commandLine is null
            ? $"{cd}; exec ${{SHELL:-/bin/sh}}"
            : $"{cd} && {EncodeSh(commandLine)}; exec ${{SHELL:-/bin/sh}}";

        return $"sh -c \"{payload}\"";
    }

    /// <summary>Encodes a command line for the sh payload. Input quote states:
    /// <c>\"</c> pairs are value quotes that must stay LITERAL at sh — re-escaped to
    /// <c>\\\"</c> so the parser (2n-backslash rule) hands sh back <c>\"</c>;
    /// bare <c>"</c> are structural quotes — escaped to <c>\"</c> so the parser
    /// reconstructs a functioning quote inside the single argv element. `$` and the
    /// backtick are backslash-escaped (the only characters sh still expands inside
    /// double quotes).</summary>
    private static string EncodeSh(string commandLine)
    {
        var sb = new System.Text.StringBuilder(commandLine.Length + 8);
        for (var i = 0; i < commandLine.Length; i++)
        {
            var ch = commandLine[i];
            if (ch == '\\' && i + 1 < commandLine.Length && commandLine[i + 1] == '"')
            {
                sb.Append('\\').Append('\\').Append('\\').Append('"');
                i++;
            }
            else if (ch == '"')
            {
                sb.Append('\\').Append('"');
            }
            else if (ch == '$')
            {
                sb.Append('\\').Append('$');
            }
            else if (ch == '`')
            {
                sb.Append('\\').Append('`');
            }
            else
            {
                sb.Append(ch);
            }
        }

        return sb.ToString();
    }

    /// <summary>Escapes a value for a PowerShell single-quoted literal ('' doubling).</summary>
    private static string PsEscapeSingleQuoted(string value) => value.Replace("'", "''");

    /// <summary>Encodes a command line for the PowerShell -Command script. Value
    /// quotes (<c>\"</c>) become backtick-escaped quotes in the script text
    /// (prefixed with a backslash so the argument parser keeps them inline);
    /// structural quotes are left bare for the wrapper's parser encoding; the
    /// backtick is doubled and `$` backtick-escaped so prompt text stays literal
    /// inside the double-quoted segments PowerShell tokenizes.</summary>
    private static string EncodePowerShell(string commandLine)
    {
        var sb = new System.Text.StringBuilder(commandLine.Length + 8);
        for (var i = 0; i < commandLine.Length; i++)
        {
            var ch = commandLine[i];
            if (ch == '\\' && i + 1 < commandLine.Length && commandLine[i + 1] == '"')
            {
                sb.Append('\\').Append('`').Append('"');
                i++;
            }
            else if (ch == '`')
            {
                sb.Append("``");
            }
            else if (ch == '$')
            {
                sb.Append('`').Append('$');
            }
            else
            {
                sb.Append(ch);
            }
        }

        return sb.ToString();
    }
}
