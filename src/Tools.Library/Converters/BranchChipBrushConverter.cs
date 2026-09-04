using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Avalonia.Media.Immutable;

namespace Tools.Library.Converters;

/// <summary>
/// Colorizes a git branch name for the Repos table's branch pill, by branch
/// family: <c>main</c> green, <c>develop</c>/<c>development</c> blue,
/// <c>release</c> orange, <c>master</c> purple, everything else muted gray.
/// The family is matched on the branch name's first path segment
/// (<c>release/1.2</c> colors as <c>release</c>), case-insensitively.
/// <para>
/// With no converter parameter the solid accent is returned (use for the text
/// and glyph); with <c>ConverterParameter=bg</c> the same color's translucent
/// tint is returned (use for the pill background), matching the 0.16 opacity
/// the other Repos chips use.
/// </para>
/// <para>
/// The brushes are cached singletons: this converter runs for every realized
/// repo row, so it must not allocate.
/// </para>
/// </summary>
public class BranchChipBrushConverter : IValueConverter
{
    private static readonly ImmutableSolidColorBrush MainAccent = new(Color.Parse("#CC228B22"));
    private static readonly ImmutableSolidColorBrush DevelopAccent = new(Color.Parse("#CC2F54EB"));
    private static readonly ImmutableSolidColorBrush ReleaseAccent = new(Color.Parse("#CCFA8C16"));
    private static readonly ImmutableSolidColorBrush MasterAccent = new(Color.Parse("#8B5CF6"));
    private static readonly ImmutableSolidColorBrush OtherAccent = new(Color.Parse("#B4B4B4"));

    private static readonly SolidColorBrush MainTint = new(Color.Parse("#CC228B22"), 0.16);
    private static readonly SolidColorBrush DevelopTint = new(Color.Parse("#CC2F54EB"), 0.16);
    private static readonly SolidColorBrush ReleaseTint = new(Color.Parse("#CCFA8C16"), 0.16);
    private static readonly SolidColorBrush MasterTint = new(Color.Parse("#8B5CF6"), 0.16);
    private static readonly SolidColorBrush OtherTint = new(Color.Parse("#B4B4B4"), 0.16);

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var head = (value as string)?.Trim().Split('/', 2)[0].ToLowerInvariant();
        return parameter is "bg"
            ? head switch
            {
                "main" => MainTint,
                "develop" or "development" => DevelopTint,
                "release" => ReleaseTint,
                "master" => MasterTint,
                _ => OtherTint,
            }
            : head switch
            {
                "main" => MainAccent,
                "develop" or "development" => DevelopAccent,
                "release" => ReleaseAccent,
                "master" => MasterAccent,
                _ => OtherAccent,
            };
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
