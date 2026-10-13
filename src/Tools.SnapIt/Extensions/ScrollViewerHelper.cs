using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Interactivity;

namespace Tools.SnapIt.Extensions;

public class ScrollViewerHelper
{
    public static bool GetFixMouseWheel(ScrollViewer scrollViewer) => scrollViewer?.GetValue(FixMouseWheelProperty) ?? false;

    public static void SetFixMouseWheel(ScrollViewer scrollViewer, bool value) => scrollViewer?.SetValue(FixMouseWheelProperty, value);

    public static readonly AttachedProperty<bool> FixMouseWheelProperty =
        AvaloniaProperty.RegisterAttached<ScrollViewerHelper, ScrollViewer, bool>("FixMouseWheel");

    static ScrollViewerHelper()
    {
        FixMouseWheelProperty.Changed.AddClassHandler<ScrollViewer>(OnFixMouseWheelChanged);
    }

    private static void OnFixMouseWheelChanged(ScrollViewer scrollViewer, AvaloniaPropertyChangedEventArgs e)
    {
        if (scrollViewer == null) return;

        scrollViewer.AddHandler(InputElement.PointerWheelChangedEvent, (s2, e2) =>
        {
            var parent = scrollViewer.Parent as Interactive;

            if (scrollViewer.HorizontalScrollBarVisibility != ScrollBarVisibility.Visible)
            {
                bool hitTopOrBottom = HitTopOrBottom(e2.Delta.Y, scrollViewer);
                if (parent is null || !hitTopOrBottom) return;
            }
            else
            {
                if (e2.Delta.Y < 0)
                {
                    scrollViewer.LineRight();
                }
                else
                {
                    scrollViewer.LineLeft();
                }
            }
        }, RoutingStrategies.Tunnel);
    }

    private static bool HitTopOrBottom(double delta, ScrollViewer scrollViewer)
    {
        var contentVerticalOffset = scrollViewer.Offset.Y;

        var atTop = contentVerticalOffset == 0;
        var movedUp = delta > 0;
        var hitTop = atTop && movedUp;

        var atBottom =
            contentVerticalOffset == scrollViewer.ScrollBarMaximum.Y;
        var movedDown = delta < 0;
        var hitBottom = atBottom && movedDown;

        return hitTop || hitBottom;
    }
}
