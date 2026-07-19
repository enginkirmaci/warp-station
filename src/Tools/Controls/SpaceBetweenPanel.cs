using Avalonia;
using Avalonia.Controls;

namespace Tools.Controls;

/// <summary>
/// Arranges children left-to-right like a wrap panel, but distributes any leftover
/// width as even gaps <em>between</em> the children of each row (the CSS flexbox
/// <c>justify-content: space-between</c> equivalent, with wrapping). Rows are packed
/// greedily: a child that no longer fits the remaining width starts a new row, and
/// every row distributes its own leftover space independently — the first child of a
/// row is flush left, the last flush right. Fixed-width sections therefore spread
/// across the full card width on large windows and reflow onto multiple aligned rows
/// on small ones, with no hardcoded breakpoints.
/// </summary>
public class SpaceBetweenPanel : Panel
{
    /// <summary>Minimum horizontal gap between two children on the same row.</summary>
    public static readonly StyledProperty<double> ColumnSpacingProperty =
        AvaloniaProperty.Register<SpaceBetweenPanel, double>(nameof(ColumnSpacing), 16);

    /// <summary>Vertical gap between rows.</summary>
    public static readonly StyledProperty<double> RowSpacingProperty =
        AvaloniaProperty.Register<SpaceBetweenPanel, double>(nameof(RowSpacing), 8);

    /// <summary>Gets or sets the minimum horizontal gap between two children on the same row.</summary>
    public double ColumnSpacing
    {
        get => GetValue(ColumnSpacingProperty);
        set => SetValue(ColumnSpacingProperty, value);
    }

    /// <summary>Gets or sets the vertical gap between rows.</summary>
    public double RowSpacing
    {
        get => GetValue(RowSpacingProperty);
        set => SetValue(RowSpacingProperty, value);
    }

    /// <inheritdoc/>
    protected override Size MeasureOverride(Size availableSize)
    {
        var rows = LayoutRows(availableSize.Width, measure: true);

        var width = 0.0;
        var height = 0.0;
        foreach (var row in rows)
        {
            var rowWidth = 0.0;
            var rowHeight = 0.0;
            foreach (var child in row)
            {
                rowWidth += child.DesiredSize.Width;
                rowHeight = Math.Max(rowHeight, child.DesiredSize.Height);
            }
            width = Math.Max(width, rowWidth);
            height += rowHeight;
        }

        height += Math.Max(0, rows.Count - 1) * RowSpacing;
        return new Size(Math.Min(width, availableSize.Width), height);
    }

    /// <inheritdoc/>
    protected override Size ArrangeOverride(Size finalSize)
    {
        var rows = LayoutRows(finalSize.Width, measure: false);

        var y = 0.0;
        foreach (var row in rows)
        {
            var rowHeight = row.Max(c => c.DesiredSize.Height);
            var contentWidth = row.Sum(c => c.DesiredSize.Width);

            // Space-between: leftover width becomes even gaps between the row's
            // children (never less than ColumnSpacing, so sections never touch).
            var gap = row.Count > 1
                ? Math.Max(ColumnSpacing, (finalSize.Width - contentWidth) / (row.Count - 1))
                : 0;

            var x = 0.0;
            foreach (var child in row)
            {
                child.Arrange(new Rect(x, y, child.DesiredSize.Width, rowHeight));
                x += child.DesiredSize.Width + gap;
            }

            y += rowHeight + RowSpacing;
        }

        return finalSize;
    }

    /// <summary>
    /// Greedily packs the visible children into rows at the given width: a child starts
    /// a new row when it no longer fits next to the current row's children (including
    /// the <see cref="ColumnSpacing"/> gap). Optionally measures the children first.
    /// </summary>
    private List<List<Control>> LayoutRows(double availableWidth, bool measure)
    {
        var rows = new List<List<Control>> { new() };
        var rowWidth = 0.0;

        foreach (var child in Children)
        {
            if (!child.IsVisible)
                continue;

            if (measure)
                child.Measure(new Size(availableWidth, double.PositiveInfinity));

            var childWidth = child.DesiredSize.Width;
            var needed = rowWidth == 0 ? childWidth : rowWidth + ColumnSpacing + childWidth;
            if (rowWidth > 0 && needed > availableWidth)
            {
                rows.Add(new List<Control>());
                rowWidth = 0;
                needed = childWidth;
            }

            rows[^1].Add(child);
            rowWidth = needed;
        }

        return rows;
    }
}
