using System.Collections.Concurrent;
using Avalonia.Markup.Xaml;
using Avalonia.Media;
using Tools.Library.Media;
using Tools.Library.Services;

namespace Tools.Library.MarkupExtensions;

/// <summary>
/// XAML markup extension that resolves an icon name (e.g. "icon-clipboard") to the
/// <see cref="Geometry"/> loaded from the Assets folder by <see cref="IconAssetLoader"/>.
/// </summary>
/// <example>
/// <code>
/// &lt;Path Data="{tools:SvgPath icon-clipboard}" /&gt;
/// </code>
/// </example>
public class SvgPathExtension : MarkupExtension
{
    private static readonly Geometry EmptyGeometry = Geometry.Parse("M0,0");

    // DataTemplates re-run ProvideValue on every container realization; parsing and
    // centering the same icon thousands of times per scroll is wasted work. Geometry
    // instances are immutable once parsed and safe to share across Path visuals
    // (identical to StaticResource geometry reuse), so the fully-processed result is
    // cached per (pathData, Center) and handed out on every subsequent realization.
    private static readonly ConcurrentDictionary<(string PathData, bool Center), Geometry> GeometryCache = new();

    public SvgPathExtension()
    {
    }

    public SvgPathExtension(string name)
    {
        Name = name;
    }

    /// <summary>
    /// Icon asset name without the .svg extension.
    /// </summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>
    /// Whether to normalize the geometry onto the square 24px design grid via
    /// <see cref="IconGeometry.CenterOnDesignGrid"/>. Content icons rely on this
    /// for uniform scaling; the window-chrome glyphs (icon-window-*) were tuned
    /// against their raw bounds and the PathIcon top-left stretch alignment, so
    /// they pass Center=False to keep their size and margins.
    /// </summary>
    public bool Center { get; set; } = true;

    public override object ProvideValue(IServiceProvider serviceProvider)
    {
        if (string.IsNullOrWhiteSpace(Name))
        {
            return EmptyGeometry;
        }

        var pathData = IconAssetLoader.GetPathData(Name);
        if (string.IsNullOrWhiteSpace(pathData))
        {
            return EmptyGeometry;
        }

        try
        {
            return GeometryCache.GetOrAdd((pathData, Center), static key =>
            {
                var geometry = Geometry.Parse(key.PathData.Replace(",", " "));
                return key.Center ? IconGeometry.CenterOnDesignGrid(geometry) : geometry;
            });
        }
        catch
        {
            return EmptyGeometry;
        }
    }
}
