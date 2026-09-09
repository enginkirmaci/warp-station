using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Avalonia.Media.Immutable;

namespace Tools.Library.Converters;

/// <summary>
/// Returns a gold brush for <see langword="true"/> (the repo is favorited, so the star
/// is filled) and a muted brush otherwise. Intended to bind to a <see cref="Repo"/>'s
/// <see cref="Repo.IsFavorite"/>, which raises change notifications from
/// <c>AddTag</c>/<c>RemoveTag</c> — binding the old way to the <c>Tags</c> collection
/// reference never re-evaluated when a tag was added or removed, leaving the star
/// painted in its pre-toggle color.
/// <para>
/// The brushes are cached immutable singletons: this converter runs for every realized
/// repo card, so it must not allocate.
/// </para>
/// </summary>
public class FavoriteStarBrushConverter : IValueConverter
{
    private static readonly ImmutableSolidColorBrush FavoriteBrush = new(Color.Parse("#F5B400"));
    private static readonly ImmutableSolidColorBrush DefaultBrush = new(Color.Parse("#8B90A0"));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
        => value is true ? FavoriteBrush : DefaultBrush;

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
