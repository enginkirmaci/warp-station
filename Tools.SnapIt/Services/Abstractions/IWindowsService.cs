using Tools.SnapIt.Common.Contracts;

namespace Tools.SnapIt.Services.Contracts;

public interface IWindowsService : IInitialize
{
    bool IsExcludedApplication(string Title, bool isKeyboard);

    bool DisableIfFullScreen();
}