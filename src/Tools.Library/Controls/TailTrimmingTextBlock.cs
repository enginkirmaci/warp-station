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
///
/// Computing the trim is expensive (a full-width layout plus an O(log n)
/// search of probe layouts, each a full shaping pass), so the result is cached
/// per (text, font, width bucket). Measure passes that repeat the same inputs
/// — row re-realization, window resize, resizer drags — then run no shaping
/// and allocate nothing.
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

    // Last computed trim result plus every input it was derived from. The key
    // is complete (text, font family/face/size, bucketed width), so a hit
    // guarantees the identical output — no explicit invalidation hooks are
    // needed: every property that can change the result is measure-affecting
    // (FullText here, the font properties via TextBlock), so any change re-runs
    // MeasureOverride and the key comparison recomputes. Foreground is
    // deliberately not part of the key: it never influences measured widths.
    private bool _trimCacheValid;
    private string? _trimKeyText;
    private FontFamily? _trimKeyFontFamily;
    private FontStyle _trimKeyFontStyle;
    private FontWeight _trimKeyFontWeight;
    private FontStretch _trimKeyFontStretch;
    private double _trimKeyFontSize;
    private double _trimKeyWidthBucket;
    private string _trimResult = string.Empty;

    protected override Size MeasureOverride(Size availableSize)
    {
        var full = FullText ?? string.Empty;

        if (double.IsInfinity(availableSize.Width) || full.Length == 0 || !IsVisible)
        {
            // Unconstrained/no content: show everything. Deliberately keeps the
            // trim cache intact — a pass here does not change any trim input.
            SetTextIfChanged(full);
            return base.MeasureOverride(availableSize);
        }

        // Bucket the constraint to whole pixels, rounding UP: ceil(w) - FitSlack
        // stays strictly below w, so a fitted string still can never spill past
        // the real constraint, while sub-pixel constraint churn (resizer drags,
        // re-measures) lands in the same bucket and becomes a cache hit.
        var widthBucket = Math.Ceiling(availableSize.Width);

        if (!IsTrimCacheHit(full, widthBucket))
        {
            var typeface = new Typeface(FontFamily, FontStyle, FontWeight, FontStretch);
            var budget = Math.Max(widthBucket - FitSlack, 0);

            double fullWidth;
            using (var layout = CreateLayout(typeface, full))
            {
                fullWidth = layout.WidthIncludingTrailingWhitespace;
            }

            _trimResult = fullWidth <= budget ? full : FitTail(typeface, full, budget);
            StoreTrimCache(full, widthBucket);
        }

        SetTextIfChanged(_trimResult);
        return base.MeasureOverride(availableSize);
    }

    /// <summary>
    /// Binary search for the longest suffix of <paramref name="full"/> that fits
    /// <paramref name="budget"/> behind the leading dots. Kept as a search over
    /// standalone suffix layouts rather than hit-testing a single layout of the
    /// full string: shaping a standalone suffix can differ by fractions of a
    /// pixel from shaping the same characters in longer context (kerning,
    /// ligatures), so hit-testing could pick a different character count — the
    /// search preserves the previous output exactly. Only runs on cache misses,
    /// and every probe layout is disposed before the next iteration.
    /// </summary>
    private string FitTail(Typeface typeface, string full, double budget)
    {
        int lo = 1, hi = full.Length, best = 0;

        while (lo <= hi)
        {
            var mid = (lo + hi) / 2;

            using var layout = CreateLayout(typeface, LeadingDots + full[^mid..]);

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

    private bool IsTrimCacheHit(string full, double widthBucket) =>
        _trimCacheValid
        && string.Equals(_trimKeyText, full, StringComparison.Ordinal)
        && Equals(_trimKeyFontFamily, FontFamily)
        && _trimKeyFontStyle == FontStyle
        && _trimKeyFontWeight.Equals(FontWeight)
        && _trimKeyFontStretch == FontStretch
        && _trimKeyFontSize.Equals(FontSize)
        && _trimKeyWidthBucket.Equals(widthBucket);

    private void StoreTrimCache(string full, double widthBucket)
    {
        _trimCacheValid = true;
        _trimKeyText = full;
        _trimKeyFontFamily = FontFamily;
        _trimKeyFontStyle = FontStyle;
        _trimKeyFontWeight = FontWeight;
        _trimKeyFontStretch = FontStretch;
        _trimKeyFontSize = FontSize;
        _trimKeyWidthBucket = widthBucket;
    }

    private void SetTextIfChanged(string value)
    {
        if (Text != value)
        {
            SetCurrentValue(TextProperty, value);
        }
    }
}
