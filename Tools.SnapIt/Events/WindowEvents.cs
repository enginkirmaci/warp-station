using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;

namespace Tools.SnapIt.Events;

public delegate void HideWindowsEvent();

public delegate bool ShowWindowsIfNecessaryEvent();

public delegate SnapAreaInfo SelectElementWithPointEvent(int x, int y);

public delegate IList<Rectangle> GetSnapAreaBoundriesEvent();