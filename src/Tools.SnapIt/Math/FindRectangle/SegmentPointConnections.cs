using System;

namespace Tools.SnapIt.Math.FindRectangle;

[Flags]
public enum SegmentPointConnections : byte
{
    None = 0,

    Top = 1,

    Bottom = 2,

    Left = 4,

    Right = 8
}