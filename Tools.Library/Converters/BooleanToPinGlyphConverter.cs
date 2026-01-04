using Microsoft.UI.Xaml.Data;

namespace Tools.Library.Converters;

/// <summary>
/// Converts a boolean pin state to the appropriate FontIcon glyph.
/// </summary>
public class BooleanToPinGlyphConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        if (value is bool isPinned)
        {
            // Pinned: Show Unpin icon (E840)
            // Unpinned: Show Pin icon (E718)
            return isPinned ? "\uE840" : "\uE718";
        }

        return "\uE718"; // Default to Pin icon
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language)
    {
        throw new NotImplementedException();
    }
}
