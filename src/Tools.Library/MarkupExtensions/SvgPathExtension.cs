using Avalonia.Markup.Xaml;
using Avalonia.Media;
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
            var normalized = pathData.Replace(",", " ");
            return Geometry.Parse(normalized);
        }
        catch
        {
            return EmptyGeometry;
        }
    }
}
