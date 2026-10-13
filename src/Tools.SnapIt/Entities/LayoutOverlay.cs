using Point = Tools.SnapIt.Graphics.Point;
using Size = Tools.SnapIt.Graphics.Size;

namespace Tools.SnapIt.Entities;

public class LayoutOverlay
{
    public Point Point { get; set; } = Point.Empty;
    public Size Size { get; set; } = Size.Empty;
    public LayoutOverlay MiniOverlay { get; set; }
}