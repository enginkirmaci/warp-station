using System.Collections;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Avalonia.Media.Immutable;
using Tools.Library.Entities;

namespace Tools.Library.Converters;

/// <summary>
/// Returns a gold brush when the bound tag collection contains the reserved
/// <c>favorites</c> tag (so the star is filled for favorited repos), and a muted brush
/// otherwise. Intended to bind to a <see cref="Repo"/>'s <see cref="Repo.Tags"/>.
/// <para>
/// The brushes are cached immutable singletons: this converter runs for every realized
/// repo card (and again whenever a card's <c>Tags</c> change), so it must not allocate.
/// </para>
/// </summary>
public class FavoriteStarBrushConverter : IValueConverter
{
    private static readonly ImmutableSolidColorBrush FavoriteBrush = new(Color.Parse("#F5B400"));
    private static readonly ImmutableSolidColorBrush DefaultBrush = new(Color.Parse("#8B90A0"));

    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var isFavorite = value is IEnumerable enumerable
            && enumerable.OfType<RepoTag>()
                .Any(t => string.Equals(t.Name, Repo.FavoritesTag, StringComparison.OrdinalIgnoreCase));

        return isFavorite ? FavoriteBrush : DefaultBrush;
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
