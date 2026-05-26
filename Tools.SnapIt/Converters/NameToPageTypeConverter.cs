using System;
using System.Linq;
using System.Reflection;

namespace Tools.SnapIt.Common.Converters;

public class NameToPageTypeConverter
{
    private static readonly Type[] PageTypes = Assembly.GetEntryAssembly().GetTypes()
        .Where(t => t.Namespace?.StartsWith("Tools.SnapIt.Views.Pages") ?? false).ToArray();

    public static Type? Convert(string pageName)
    {
        pageName = pageName.Trim().ToLower() + "page";

        return PageTypes.FirstOrDefault(singlePageType => singlePageType.Name.ToLower() == pageName);
    }
}