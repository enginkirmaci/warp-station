namespace Tools.Services.Abstractions;

/// <summary>
/// Launches multiple OpenCode terminal instances and tiles them across the active
/// screen in a grid (e.g. 6 instances -> a 3x2 grid), positioning each window into a
/// distinct cell using the SnapIt Win32 primitives. Bridges process launching
/// (<see cref="Tools.Library.Services.Abstractions.IProcessLauncher"/>) and window
/// positioning (<c>Tools.SnapIt.Services.Abstractions.IWinApiService</c>).
/// </summary>
public interface IOpenCodeGridLauncher
{
    /// <summary>
    /// Launches <paramref name="count"/> terminal windows running opencode and arranges
    /// them in a grid computed from the active screen's working area.
    /// </summary>
    /// <param name="terminalExe">The terminal executable to host opencode (e.g. <c>wt</c>).</param>
    /// <param name="openCodeExe">The opencode executable or command.</param>
    /// <param name="folderPath">The folder to open the terminals in.</param>
    /// <param name="model">The opencode model string (may be empty).</param>
    /// <param name="prompt">An optional start prompt (may be empty).</param>
    /// <param name="count">How many instances to launch and tile.</param>
    Task LaunchAsync(string terminalExe, string openCodeExe, string folderPath, string model, string prompt, int count);
}
