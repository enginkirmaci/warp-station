namespace Tools.Library.Converters;

public class NameToPageTypeConverter
{
    private static readonly Type[] PageTypes = Assembly.GetEntryAssembly().GetTypes()
        .Where(t => t.Namespace?.StartsWith("Tools.Views.Pages") ?? false).ToArray();

    public static Type? Convert(string pageName)
    {
        pageName = pageName.Trim().ToLower() + "page";

        return PageTypes.FirstOrDefault(singlePageType => singlePageType.Name.ToLower() == pageName);
    }
}