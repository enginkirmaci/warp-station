using Tools.SnapIt.Common.Entities;

namespace Tools.SnapIt.Common.Events;

public delegate void SnappingCancelEvent();

public delegate void MoveWindowEvent(SnapAreaInfo snapAreaInfo, bool isLeftClick);