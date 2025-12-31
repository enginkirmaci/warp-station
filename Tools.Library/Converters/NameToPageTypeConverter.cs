using System;
using System.Linq;

namespace Tools.Library.Converters;

public static class NameToPageTypeConverter
{
    private static readonly Type[] PageTypes;

    static NameToPageTypeConverter()
    {
        PageTypes = AppDomain.CurrentDomain.GetAssemblies()
            .SelectMany(a => a.GetTypes())
            .Where(t => t.IsClass && !t.IsAbstract && t.Name.EndsWith("Page"))
            .ToArray();
    }

    public static Type Convert(string pageName)
    {
        if (string.IsNullOrWhiteSpace(pageName))
            return null!;

        return PageTypes.FirstOrDefault(t => t.Name.Equals(pageName, StringComparison.OrdinalIgnoreCase) 
                                          || t.Name.Equals(pageName + "Page", StringComparison.OrdinalIgnoreCase))!;
    }
}