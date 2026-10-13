using Point = Tools.SnapIt.Graphics.Point;

namespace Tools.SnapIt.Json;

public class PointToStringJsonConverter : FloatPairJsonConverter<Point>
{
    protected override Point Create() => new Point();

    protected override float GetFirst(Point value) => value.X;

    protected override float GetSecond(Point value) => value.Y;

    protected override void SetFirst(Point value, float first) => value.X = first;

    protected override void SetSecond(Point value, float second) => value.Y = second;
}
