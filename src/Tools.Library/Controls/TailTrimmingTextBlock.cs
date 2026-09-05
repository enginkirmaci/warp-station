using Avalonia;
using Avalonia.Media;
using Avalonia.Media.TextFormatting;

namespace Tools.Library.Controls;

/// <summary>
/// TextBlock that, when too narrow, cuts the BEGINNING of the text and keeps
/// the tail, prefixed with ".." — the opposite of the built-in end-trimming.
/// Used for git branch pills: the tail of a branch name is the part that
/// tells branches apart. The full string is carried by <see cref="FullText"/>;
/// the visual <see cref="TextBlock.Text"/> is managed by the control itself.
///
/// Truncation needs a finite width constraint: the hosting pill passes one
/// through implicitly unreliable (its own content is measured unconstrained),
/// so the XAML caps this control with an explicit MaxWidth — which also keeps
/// short pills hugging their text, since MaxWidth caps without forcing width.
/// </summary>
public class TailTrimmingTextBlock : TextBlock
{
    private const string LeadingDots = "..";
    // Shaping/rendering engines differ by a fraction of a pixel; keep a little
    // slack so the fitted string never spills past the constraint.
    private const double FitSlack = 1.0;

    public static readonly StyledProperty<string> FullTextProperty =
        AvaloniaProperty.Register<TailTrimmingTextBlock, string>(nameof(FullText), string.Empty);

    static TailTrimmingTextBlock()
    {
        AffectsMeasure<TailTrimmingTextBlock>(FullTextProperty);
    }

    public string FullText
    {
        get => GetValue(FullTextProperty);
        set => SetValue(FullTextProperty, value);
    }

    protected override Size MeasureOverride(Size availableSize)
    {
        var full = FullText ?? string.Empty;

        if (double.IsInfinity(availableSize.Width) || full.Length == 0 || !IsVisible)
        {
            SetTextIfChanged(full);
            return base.MeasureOverride(availableSize);
        }

        var typeface = new Typeface(FontFamily, FontStyle, FontWeight, FontStretch);
        double fullWidth = CreateLayout(typeface, full).WidthIncludingTrailingWhitespace;
        var budget = Math.Max(availableSize.Width - FitSlack, 0);

        SetTextIfChanged(fullWidth <= budget ? full : FitTail(typeface, full, budget));
        return base.MeasureOverride(availableSize);
    }

    /// <summary>
    /// Binary search for the longest suffix of <paramref name="full"/> that fits
    /// <paramref name="budget"/> behind the leading dots. Branch names are short,
    /// so the log-time probing with throwaway layouts is negligible.
    /// </summary>
    private string FitTail(Typeface typeface, string full, double budget)
    {
        int lo = 1, hi = full.Length, best = 0;

        while (lo <= hi)
        {
            var mid = (lo + hi) / 2;
            var layout = CreateLayout(typeface, LeadingDots + full[^mid..]);

            if (layout.WidthIncludingTrailingWhitespace <= budget)
            {
                best = mid;
                lo = mid + 1;
            }
            else
            {
                hi = mid - 1;
            }
        }

        return best > 0
            ? LeadingDots + full[^best..]
            : LeadingDots; // even one tail char + dots cannot fit
    }

    private TextLayout CreateLayout(Typeface typeface, string text) => new(
        text,
        typeface,
        FontSize,
        Foreground,
        maxWidth: double.PositiveInfinity);

    private void SetTextIfChanged(string value)
    {
        if (Text != value)
        {
            SetCurrentValue(TextProperty, value);
        }
    }
}
