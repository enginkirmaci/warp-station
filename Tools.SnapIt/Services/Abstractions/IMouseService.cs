using Tools.SnapIt.Common.Contracts;
using Tools.SnapIt.Common.Events;

namespace Tools.SnapIt.Services.Contracts;

public interface IMouseService : IInitialize
{
    void Interrupt();

    event HideWindowsEvent HideWindows;

    event MoveWindowEvent MoveWindow;

    event SnappingCancelEvent SnappingCancelled;

    event ShowWindowsIfNecessaryEvent ShowWindowsIfNecessary;

    event SelectElementWithPointEvent SelectElementWithPoint;
}