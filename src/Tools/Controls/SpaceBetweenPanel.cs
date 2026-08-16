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
    /// <summary>
    /// Layout scratch buffers, reused across measure/arrange passes so scrolling
    /// (which recycles and re-lays-out many panels) does not allocate per pass:
    /// <see cref="_visibleChildren"/> holds the visible children in tree order and
    /// <see cref="_rowCounts"/> the number of children packed into each row.
    /// </summary>
    private readonly List<Control> _visibleChildren = new();
    private readonly List<int> _rowCounts = new();

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
        LayoutRows(availableSize.Width, measure: true);

        var width = 0.0;
        var height = 0.0;
        var offset = 0;
        foreach (var count in _rowCounts)
        {
            var rowWidth = 0.0;
            var rowHeight = 0.0;
            for (var i = 0; i < count; i++)
            {
                var child = _visibleChildren[offset + i];
                rowWidth += child.DesiredSize.Width;
                rowHeight = Math.Max(rowHeight, child.DesiredSize.Height);
            }
            offset += count;
            width = Math.Max(width, rowWidth);
            height += rowHeight;
        }

        height += Math.Max(0, _rowCounts.Count - 1) * RowSpacing;
        return new Size(Math.Min(width, availableSize.Width), height);
    }

    /// <inheritdoc/>
    protected override Size ArrangeOverride(Size finalSize)
    {
        LayoutRows(finalSize.Width, measure: false);

        var y = 0.0;
        var offset = 0;
        foreach (var count in _rowCounts)
        {
            var rowHeight = 0.0;
            var contentWidth = 0.0;
            for (var i = 0; i < count; i++)
            {
                var child = _visibleChildren[offset + i];
                rowHeight = Math.Max(rowHeight, child.DesiredSize.Height);
                contentWidth += child.DesiredSize.Width;
            }

            // Space-between: leftover width becomes even gaps between the row's
            // children (never less than ColumnSpacing, so sections never touch).
            var gap = count > 1
                ? Math.Max(ColumnSpacing, (finalSize.Width - contentWidth) / (count - 1))
                : 0;

            var x = 0.0;
            for (var i = 0; i < count; i++)
            {
                var child = _visibleChildren[offset + i];
                child.Arrange(new Rect(x, y, child.DesiredSize.Width, rowHeight));
                x += child.DesiredSize.Width + gap;
            }

            offset += count;
            y += rowHeight + RowSpacing;
        }

        return finalSize;
    }

    /// <summary>
    /// Greedily packs the visible children into rows at the given width: a child starts
    /// a new row when it no longer fits next to the current row's children (including
    /// the <see cref="ColumnSpacing"/> gap). Optionally measures the children first.
    /// </summary>
    private void LayoutRows(double availableWidth, bool measure)
    {
        _visibleChildren.Clear();
        _rowCounts.Clear();

        var rowWidth = 0.0;
        var countInRow = 0;

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
                _rowCounts.Add(countInRow);
                countInRow = 0;
                rowWidth = 0;
                needed = childWidth;
            }

            _visibleChildren.Add(child);
            countInRow++;
            rowWidth = needed;
        }

        if (countInRow > 0)
            _rowCounts.Add(countInRow);
    }
}
