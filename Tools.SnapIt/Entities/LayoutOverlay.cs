using Tools.SnapIt.Common.Graphics;
using Point = Tools.SnapIt.Common.Graphics.Point;
using Size = Tools.SnapIt.Common.Graphics.Size;

namespace Tools.SnapIt.Common.Entities;

public class LayoutOverlay
{
    public Point Point { get; set; } = Point.Empty;
    public Size Size { get; set; } = Size.Empty;
    public LayoutOverlay MiniOverlay { get; set; }
}