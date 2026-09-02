namespace Tools.Library.Services.Abstractions;

public interface IProcessLauncher
{
    /// <summary>
    /// Launches a process. When <paramref name="stripElectronEnvironment"/> is set, the
    /// child starts with ELECTRON_RUN_AS_NODE removed: the variable leaks in when Tools
    /// itself was launched from an Electron host such as VS Code, and makes
    /// Electron-packaged CLIs degrade to a bare Node REPL.
    /// </summary>
    void StartProcess(string fileName, string? arguments = null, bool hidden = false, bool stripElectronEnvironment = false);

    Task StartProcessAsync(string fileName, string? arguments = null, bool hidden = false, bool stripElectronEnvironment = false);
}