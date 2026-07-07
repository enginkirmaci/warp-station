using System.Collections;
using System.Globalization;
using Avalonia.Data.Converters;
using Avalonia.Media;
using Tools.Library.Entities;

namespace Tools.Library.Converters;

/// <summary>
/// Returns a gold brush when the bound tag collection contains the reserved
/// <c>favorites</c> tag (so the star is filled for favorited repos), and a muted brush
/// otherwise. Intended to bind to a <see cref="Repo"/>'s <see cref="Repo.Tags"/>.
/// </summary>
public class FavoriteStarBrushConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture)
    {
        var isFavorite = value is IEnumerable enumerable
            && enumerable.OfType<RepoTag>()
                .Any(t => string.Equals(t.Name, Repo.FavoritesTag, StringComparison.OrdinalIgnoreCase));

        return isFavorite
            ? new SolidColorBrush(Color.Parse("#F5B400"))
            : new SolidColorBrush(Color.Parse("#8B90A0"));
    }

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture)
        => throw new NotSupportedException();
}
