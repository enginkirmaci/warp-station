using Tools.SnapIt.Common.Contracts;
using Tools.SnapIt.Common.Entities;
using Tools.SnapIt.Common.Events;

namespace Tools.SnapIt.Services.Contracts;

public delegate void SnapStartStopEvent();

public delegate void ChangeLayoutEvent(SnapScreen snapScreen, Layout layout);

public interface IKeyboardService : IInitialize
{
    event SnappingCancelEvent SnappingCancelled;

    event SnapStartStopEvent SnapStartStop;

    event MoveWindowEvent MoveWindow;

    event GetSnapAreaBoundriesEvent GetSnapAreaBoundries;

    event ChangeLayoutEvent ChangeLayout;

    void SetSnappingStopped();
}