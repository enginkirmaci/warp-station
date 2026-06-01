using Tools.SnapIt.Entities;

namespace Tools.SnapIt.Events;

public delegate void SnappingCancelEvent();

public delegate void MoveWindowEvent(SnapAreaInfo snapAreaInfo, bool isLeftClick);