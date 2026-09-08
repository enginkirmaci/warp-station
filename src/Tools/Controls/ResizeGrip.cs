using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.Shapes;
using Avalonia.Input;
using Avalonia.Layout;
using Avalonia.Media;

namespace Tools.Controls;

/// <summary>The drag axis a resize grip works on (mirrors PanelResizeAxis).</summary>
public enum ResizeGripOrientation
{
    /// <summary>Vertical drag adjusts height: a full-width horizontal band (bottom panels).</summary>
    Vertical,

    /// <summary>Horizontal drag adjusts width: a full-height vertical band (right-docked drawers).</summary>
    Horizontal
}

/// <summary>
/// The shared "three grip dots" divider between resizable panels. Deliberately IS a
/// Border: the owning views resolve it by name as a Border and hand it to
/// <c>PanelResizeController</c> in code-behind — the whole band (not just the dots)
/// must stay the pointer target, so the control only supplies the transparent hit
/// background, the axis-matching cursor and the centered dots. Band size and edge
/// alignment are layout decisions of each site and stay per-instance in XAML
/// (the 16px band is a user-tuned value).
/// </summary>
public sealed class ResizeGrip : Border
{
    private const string DotBrushKey = "MutedForegroundBrush";

    /// <summary>Styled property for the drag axis this grip resizes along.</summary>
    public static readonly StyledProperty<ResizeGripOrientation> OrientationProperty =
        AvaloniaProperty.Register<ResizeGrip, ResizeGripOrientation>(
            nameof(Orientation),
            defaultValue: ResizeGripOrientation.Vertical);

    private readonly StackPanel _dots;

    static ResizeGrip()
    {
        // Transparent (not unset) so the whole band hit-tests as a drag surface,
        // exactly like the Background="Transparent" the sites used to set.
        BackgroundProperty.OverrideDefaultValue<ResizeGrip>(Brushes.Transparent);
    }

    public ResizeGrip()
    {
        // Three 3px dots, 3px apart, centered — per orientation a horizontal row
        // (vertical drag) or a vertical column (horizontal drag). The fill is bound
        // to the shared theme brush, the code equivalent of {DynamicResource}, so
        // theme swaps still retint the dots.
        _dots = new StackPanel
        {
            HorizontalAlignment = HorizontalAlignment.Center,
            VerticalAlignment = VerticalAlignment.Center,
            Spacing = 3
        };
        for (var i = 0; i < 3; i++)
        {
            var dot = new Ellipse { Width = 3, Height = 3 };
            dot.Bind(Shape.FillProperty, this.GetResourceObservable(DotBrushKey));
            _dots.Children.Add(dot);
        }

        Child = _dots;
        UpdateForOrientation();
    }

    /// <summary>The drag axis this grip resizes along.</summary>
    public ResizeGripOrientation Orientation
    {
        get => GetValue(OrientationProperty);
        set => SetValue(OrientationProperty, value);
    }

    /// <inheritdoc/>
    protected override void OnPropertyChanged(AvaloniaPropertyChangedEventArgs change)
    {
        base.OnPropertyChanged(change);
        if (change.Property == OrientationProperty)
        {
            UpdateForOrientation();
        }
    }

    private void UpdateForOrientation()
    {
        var vertical = Orientation == ResizeGripOrientation.Vertical;
        _dots.Orientation = vertical
            ? Avalonia.Layout.Orientation.Horizontal
            : Avalonia.Layout.Orientation.Vertical;
        Cursor = new Cursor(vertical
            ? StandardCursorType.SizeNorthSouth
            : StandardCursorType.SizeWestEast);
    }
}
