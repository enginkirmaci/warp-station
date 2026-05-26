using Tools.SnapIt.Common.Graphics;

namespace Tools.SnapIt.Common.Entities;

public class ActiveWindow
{
    public nint Handle { get; set; }
    public string Title { get; set; }
    public Rectangle Boundry { get; set; }
    public Dpi Dpi { get; set; }

    public static readonly ActiveWindow Empty = new ActiveWindow();
}