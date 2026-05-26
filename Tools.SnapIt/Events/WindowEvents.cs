using Tools.SnapIt.Common.Entities;
using Tools.SnapIt.Common.Graphics;

namespace Tools.SnapIt.Common.Events;

public delegate void HideWindowsEvent();

public delegate bool ShowWindowsIfNecessaryEvent();

public delegate SnapAreaInfo SelectElementWithPointEvent(int x, int y);

public delegate IList<Rectangle> GetSnapAreaBoundriesEvent();