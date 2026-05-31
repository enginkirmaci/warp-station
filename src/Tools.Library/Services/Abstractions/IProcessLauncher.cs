namespace Tools.Library.Services.Abstractions;

public interface IProcessLauncher
{
    void StartProcess(string fileName, string? arguments = null, bool hidden = false);
    Task StartProcessAsync(string fileName, string? arguments = null, bool hidden = false);
}