using Avalonia;
using Avalonia.Controls;

namespace Tools.Views.Controls;

public class ResponsiveWrapPanel : Panel
{
    public static readonly StyledProperty<double> MinItemWidthProperty =
        AvaloniaProperty.Register<ResponsiveWrapPanel, double>(
            nameof(MinItemWidth), 240d);

    public static readonly StyledProperty<double> SpacingProperty =
        AvaloniaProperty.Register<ResponsiveWrapPanel, double>(
            nameof(Spacing), 12d);

    static ResponsiveWrapPanel()
    {
        AffectsMeasure<ResponsiveWrapPanel>(MinItemWidthProperty, SpacingProperty);
        AffectsArrange<ResponsiveWrapPanel>(MinItemWidthProperty, SpacingProperty);
    }

    public double MinItemWidth
    {
        get => GetValue(MinItemWidthProperty);
        set => SetValue(MinItemWidthProperty, value);
    }

    public double Spacing
    {
        get => GetValue(SpacingProperty);
        set => SetValue(SpacingProperty, value);
    }

    private int CalculateColumns(double availableWidth)
    {
        double spacing = Math.Max(0, Spacing);
        double minWidth = Math.Max(1, MinItemWidth);
        int columns = (int)Math.Floor((availableWidth + spacing) / (minWidth + spacing));
        return Math.Max(1, columns);
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        var children = Children;
        int count = children.Count;
        if (count == 0)
            return new Size(0, 0);

        double spacing = Math.Max(0, Spacing);
        int columns = CalculateColumns(availableSize.Width);
        double itemWidth = (availableSize.Width - (columns - 1) * spacing) / columns;

        double totalHeight = 0;
        double rowMaxHeight = 0;

        for (int i = 0; i < count; i++)
        {
            children[i].Measure(new Size(itemWidth, double.PositiveInfinity));
            rowMaxHeight = Math.Max(rowMaxHeight, children[i].DesiredSize.Height);

            if ((i + 1) % columns == 0 || i == count - 1)
            {
                totalHeight += rowMaxHeight;
                if (i < count - 1)
                {
                    totalHeight += spacing;
                }
                rowMaxHeight = 0;
            }
        }

        return new Size(availableSize.Width, totalHeight);
    }

    protected override Size ArrangeOverride(Size finalSize)
    {
        var children = Children;
        int count = children.Count;
        if (count == 0)
            return finalSize;

        double spacing = Math.Max(0, Spacing);
        int columns = CalculateColumns(finalSize.Width);
        double itemWidth = (finalSize.Width - (columns - 1) * spacing) / columns;

        double y = 0;
        double rowMaxHeight = 0;
        int col = 0;
        int rowStart = 0;

        for (int i = 0; i < count; i++)
        {
            children[i].Arrange(new Rect(
                col * (itemWidth + spacing),
                y,
                itemWidth,
                children[i].DesiredSize.Height));

            rowMaxHeight = Math.Max(rowMaxHeight, children[i].DesiredSize.Height);
            col++;

            bool rowEnd = (i + 1) % columns == 0;
            bool last = i == count - 1;
            if (rowEnd || last)
            {
                y += rowMaxHeight + spacing;
                rowMaxHeight = 0;
                col = 0;
                rowStart = i + 1;
            }
        }

        return finalSize;
    }
}
