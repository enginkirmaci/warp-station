namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Intent-level launch operations behind the repo launch buttons and the OpenCode
/// launches: owns the "which executable, which arguments, which fallback" decisions —
/// terminal / IDE / CLI resolution (<see cref="ExecutableDefaults"/>), terminal-specific
/// argument shapes (<see cref="Formatters.TerminalArgumentFormatter"/>) and the DevTools
/// pipe launch with its direct-spawn fallback — so ViewModels keep only the command
/// wiring, busy flags and toasts.
/// </summary>
public interface ITerminalLauncher
{
    /// <summary>
    /// Opens a folder with its shell association (file manager / explorer). A null or
    /// whitespace path is a no-op.
    /// </summary>
    void OpenFolder(string? folderPath);

    /// <summary>
    /// Opens a solution: the configured IDE when one resolves, otherwise — on Windows
    /// only — the .sln shell association. On other platforms without a detected .NET IDE
    /// a warning is logged and nothing is launched.
    /// </summary>
    /// <param name="solutionPath">The .sln file to open.</param>
    /// <param name="configuredIdeExecutable">The configured IDE executable, if any.</param>
    void OpenSolution(string solutionPath, string? configuredIdeExecutable);

    /// <summary>
    /// Opens a folder in VS Code, with the configured profile when set
    /// (<c>--profile &lt;name&gt;</c>). Routes through the DevTools service (named pipe)
    /// so the launch is non-elevated even when Tools runs as admin; falls back to a
    /// direct spawn when the pipe is unavailable.
    /// </summary>
    /// <param name="folderPath">The folder to open.</param>
    /// <param name="configuredExecutable">The configured VS Code executable, if any.</param>
    /// <param name="profile">The configured VS Code profile, or null/empty for the default profile.</param>
    Task OpenInVSCodeAsync(string folderPath, string? configuredExecutable, string? profile);

    /// <summary>
    /// Opens a folder in the configured terminal (no command runs in it). A no-op when
    /// no terminal resolves.
    /// </summary>
    /// <param name="folderPath">The folder to open the terminal in.</param>
    /// <param name="configuredTerminalExecutable">The configured terminal executable, if any.</param>
    void OpenFolderInTerminal(string folderPath, string? configuredTerminalExecutable);

    /// <summary>
    /// Opens a folder with zcode: the AppImage desktop package (no interactive CLI
    /// runtime) is launched directly on the folder like VS Code, a standalone CLI binary
    /// runs inside the configured terminal instead. A no-op when no terminal resolves for
    /// the CLI variant.
    /// </summary>
    /// <param name="folderPath">The folder to open zcode in.</param>
    /// <param name="configuredZCodeExecutable">The configured zcode executable, if any.</param>
    /// <param name="configuredTerminalExecutable">The configured terminal executable, if any.</param>
    void OpenZCode(string folderPath, string? configuredZCodeExecutable, string? configuredTerminalExecutable);

    /// <summary>
    /// Launches <paramref name="count"/> opencode instances as plain terminal windows,
    /// each opening in <paramref name="folderPath"/> and running
    /// <paramref name="openCodeExe"/> with the model and start prompt — the non-tiled
    /// counterpart of the grid launcher, with the same command line.
    /// </summary>
    /// <param name="terminalExe">The resolved terminal executable to host opencode.</param>
    /// <param name="openCodeExe">The resolved opencode executable or quoted command token.</param>
    /// <param name="folderPath">The folder to open the terminals in.</param>
    /// <param name="model">The opencode model (may be empty).</param>
    /// <param name="prompt">An optional start prompt (may be empty).</param>
    /// <param name="count">How many instances to launch (clamped to at least 1).</param>
    void LaunchOpenCode(string terminalExe, string openCodeExe, string folderPath, string model, string prompt, int count);
}
