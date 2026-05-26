using Tools.SnapIt.Contracts;

namespace Tools.SnapIt.Services.Abstractions;

public interface IWindowsService : IInitialize
{
    bool IsExcludedApplication(string Title);

    bool DisableIfFullScreen();
}