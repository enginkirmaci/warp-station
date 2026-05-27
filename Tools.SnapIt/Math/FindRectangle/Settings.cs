using Tools.SnapIt.Entities;
using Point = System.Drawing.Point;
using Size = System.Drawing.Size;

namespace Tools.SnapIt.Math.FindRectangle;

// Cyotek Slice Rectangle Sample
// Copyright (c) 2013 Cyotek. All Rights Reserved.
// http://cyotek.com
// http://cyotek.com/blog/dividing-up-a-rectangle-based-on-pairs-of-points-using-csharp

// If you use this code in your applications, attribution or donations are welcome.

public class Settings
{
    #region Constructors

    public Settings()
    {
        Segments = [];
    }

    #endregion Constructors

    #region Properties

    public List<Segment> Segments { get; set; }

    public Size Size { get; set; }

    private IDictionary<Point, SegmentPoint> Points { get; set; }

    private HashSet<Rectangle> Rectangles { get; set; }

    #endregion Properties

    #region Members

    public void Calculate()
    {
        CalculatePoints();
        CalculateRectangles();
    }

    public SegmentPoint[] GetPoints()
    {
        return Points != null ? Points.Values.ToArray() : [];
    }

    public Rectangle[] GetRectangles()
    {
        return Rectangles != null ? Rectangles.ToArray() : [];
    }

    private void CalculatePoints()
    {
        List<Segment> segments;

        segments = [];
        Points = new Dictionary<Point, SegmentPoint>();

        //add segments representing the edges
        segments.Add(new Segment { Location = new Point(0, 0), EndLocation = new Point(Size.Width, 0), Orientation = SplitDirection.Horizontal });
        segments.Add(new Segment { Location = new Point(0, Size.Height), EndLocation = new Point(Size.Width, Size.Height), Orientation = SplitDirection.Horizontal });
        segments.Add(new Segment { Location = new Point(0, 0), EndLocation = new Point(0, Size.Height), Orientation = SplitDirection.Vertical });
        segments.Add(new Segment { Location = new Point(Size.Width, 0), EndLocation = new Point(Size.Width, Size.Height), Orientation = SplitDirection.Vertical });

        // add the rest of the segments
        segments.AddRange(Segments);

        FixPoints(segments);

        segments.Sort((a, b) =>
        {
            int result = a.Location.X.CompareTo(b.Location.X);
            if (result == 0)
                result = a.Location.Y.CompareTo(b.Location.Y);
            return result;
        });

        foreach (Segment segment in segments)
        {
            Segment currentSegment;

            // add the segment points
            UpdatePoint(segment.Location, segment.Orientation == SplitDirection.Horizontal ? SegmentPointConnections.Left : SegmentPointConnections.Top);
            UpdatePoint(segment.EndLocation, segment.Orientation == SplitDirection.Horizontal ? SegmentPointConnections.Right : SegmentPointConnections.Bottom);

            // calculate any intersecting points
            currentSegment = segment;
            foreach (Segment otherSegment in segments.Where(s => s != currentSegment))
            {
                Point intersection;

                intersection = Intersection.FindLineIntersection(segment.Location, segment.EndLocation, otherSegment.Location, otherSegment.EndLocation);
                if (intersection != Point.Empty)
                {
                    SegmentPointConnections flags;

                    flags = SegmentPointConnections.None;
                    if (intersection != segment.Location && intersection != segment.EndLocation)
                    {
                        if (segment.Orientation == SplitDirection.Horizontal)
                            flags |= SegmentPointConnections.Left | SegmentPointConnections.Right;
                        else
                            flags |= SegmentPointConnections.Top | SegmentPointConnections.Bottom;
                    }
                    else if (intersection != otherSegment.Location && intersection != otherSegment.EndLocation)
                    {
                        if (otherSegment.Orientation == SplitDirection.Horizontal)
                            flags |= SegmentPointConnections.Left | SegmentPointConnections.Right;
                        else
                            flags |= SegmentPointConnections.Top | SegmentPointConnections.Bottom;
                    }

                    if (flags != SegmentPointConnections.None)
                        UpdatePoint(intersection, flags);
                }
            }
        }
    }

    private void FixPoints(List<Segment> segments)
    {
        var pointXs = new List<int>(segments.Count * 2);
        var pointYs = new List<int>(segments.Count * 2);

        for (int i = 0; i < segments.Count; i++)
        {
            pointXs.Add(segments[i].Location.X);
            pointYs.Add(segments[i].Location.Y);
            pointXs.Add(segments[i].EndLocation.X);
            pointYs.Add(segments[i].EndLocation.Y);
        }

        var tolerance = 4;

        for (int i = 4; i < segments.Count; i++)
        {
            var segment = segments[i];

            int? locationX = null;
            int? locationY = null;
            int? endLocationX = null;
            int? endLocationY = null;

            for (int j = 0; j < pointXs.Count; j++)
            {
                if (!locationX.HasValue && NumberInRange(segment.Location.X, pointXs[j], tolerance))
                    locationX = pointXs[j];
                if (!locationY.HasValue && NumberInRange(segment.Location.Y, pointYs[j], tolerance))
                    locationY = pointYs[j];
                if (!endLocationX.HasValue && NumberInRange(segment.EndLocation.X, pointXs[j], tolerance))
                    endLocationX = pointXs[j];
                if (!endLocationY.HasValue && NumberInRange(segment.EndLocation.Y, pointYs[j], tolerance))
                    endLocationY = pointYs[j];
            }

            segment.Location = new Point(
                locationX ?? segment.Location.X,
                locationY ?? segment.Location.Y);

            segment.EndLocation = new Point(
                endLocationX ?? segment.EndLocation.X,
                endLocationY ?? segment.EndLocation.Y);
        }
    }

    private bool NumberInRange(int value, int compare, int tolerance)
    {
        return value - tolerance < compare && compare < value + tolerance;
    }

    private void CalculateRectangles()
    {
        SegmentPoint[] horizontalPoints;
        SegmentPoint[] verticalPoints;

        Rectangles = [];
        var points = Points.Values;
        horizontalPoints = points.OrderBy(p => p.X).ToArray();
        verticalPoints = points.OrderBy(p => p.Y).ToArray();

        foreach (SegmentPoint topLeft in points)
        {
            if (!topLeft.Connections.HasFlag(SegmentPointConnections.Left | SegmentPointConnections.Top))
                continue;

            SegmentPoint topRight;
            SegmentPoint bottomLeft;

            topRight = Array.Find(horizontalPoints, p => p.X > topLeft.X && p.Y == topLeft.Y && p.Connections.HasFlag(SegmentPointConnections.Right | SegmentPointConnections.Top));
            bottomLeft = Array.Find(verticalPoints, p => p.X == topLeft.X && p.Y > topLeft.Y && p.Connections.HasFlag(SegmentPointConnections.Left | SegmentPointConnections.Bottom));

            if (topRight != null && bottomLeft != null)
            {
                SegmentPoint bottomRight;

                bottomRight = Array.Find(horizontalPoints, p => p.X == topRight.X && p.Y == bottomLeft.Y && p.Connections.HasFlag(SegmentPointConnections.Right | SegmentPointConnections.Bottom));

                if (bottomRight != null)
                {
                    Rectangle rectangle;

                    rectangle = new Rectangle(topLeft, bottomRight, bottomRight.X - topLeft.X, bottomRight.Y - topLeft.Y);

                    Rectangles.Add(rectangle);
                }
            }
        }
    }

    private void UpdatePoint(Point location, SegmentPointConnections connections)
    {
        SegmentPoint point;

        if (!Points.TryGetValue(location, out point))
        {
            point = new SegmentPoint { Location = location, Connections = connections };
            Points.Add(point.Location, point);
        }
        else if (!point.Connections.HasFlag(connections))
            point.Connections |= connections;
    }

    #endregion Members
}