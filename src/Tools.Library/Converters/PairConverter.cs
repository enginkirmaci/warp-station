using System.Globalization;
using Avalonia.Data.Converters;
using Tools.Library.Entities;

namespace Tools.Library.Converters;

/// <summary>
/// Combines two bound values (a parent <see cref="Repo"/> and a child tag string) into
/// a strongly-typed <see cref="Tuple{T1, T2}"/> so a single command can receive both
/// via <c>CommandParameter</c>. Intended for <c>MultiBinding</c> with exactly two child
/// bindings inside a <see cref="Repo"/> data template (e.g. quick-add tag chips).
/// </summary>
/// <remarks>
/// A plain <c>Tuple.Create(object, object)</c> would produce a
/// <c>Tuple&lt;object, object&gt;</c>, which <c>RelayCommand&lt;Tuple&lt;Repo, string&gt;&gt;</c>
/// rejects at attach time with an <see cref="ArgumentException"/>. This converter casts
/// the boxed <see cref="MultiBinding"/> values to their concrete types before constructing
/// the tuple so the generated relay command accepts the parameter.
/// </remarks>
public class PairConverter : IMultiValueConverter
{
    public object? Convert(IList<object?> values, Type targetType, object? parameter, CultureInfo culture)
    {
        if (values.Count < 2)
            return null;

        if (values[0] is Repo repo && values[1] is string tag)
            return Tuple.Create(repo, tag);

        return null;
    }
}
