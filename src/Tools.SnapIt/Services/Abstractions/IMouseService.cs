using Tools.SnapIt.Contracts;
using Tools.SnapIt.Events;

namespace Tools.SnapIt.Services.Abstractions;

public interface IMouseService : IInitialize
{
    void Interrupt();

    event HideWindowsEvent HideWindows;

    event MoveWindowEvent MoveWindow;

    event SnappingCancelEvent SnappingCancelled;

    event ShowWindowsIfNecessaryEvent ShowWindowsIfNecessary;

    event SelectElementWithPointEvent SelectElementWithPoint;
}