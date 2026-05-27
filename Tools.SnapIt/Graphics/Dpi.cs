namespace Tools.SnapIt.Graphics;

public class Dpi
{
    public float X;
    public float Y;

    public static Dpi Default
    { get { return new Dpi { X = 1.0f, Y = 1.0f }; } }

    public override string ToString()
    {
        return $"X:{X}, Y:{Y}";
    }

    public override bool Equals(Object obj)
    {
        if (obj is Dpi d)
        {
            return (X == d.X) && (Y == d.Y);
        }
        return false;
    }

    public override int GetHashCode()
    {
        return (X.GetHashCode() << 2) ^ Y.GetHashCode();
    }

    public static bool operator ==(Dpi left, Dpi right)
    {
        if (left is null)
            return right is null;
        return left.Equals(right);
    }

    public static bool operator !=(Dpi left, Dpi right)
    {
        return !(left == right);
    }
}