using Tools.SnapIt.Entities;
using Point = System.Drawing.Point;

namespace Tools.SnapIt.Math.FindRectangle;

public class Segment
{
    public Point Location { get; set; }
    public Point EndLocation { get; set; }
    public SplitDirection Orientation { get; set; }

    public override string ToString()
    {
        return $"{Location} x {EndLocation}";
    }
}