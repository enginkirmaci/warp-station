using System.Globalization;
using System.Text;

namespace Tools.Library.Extensions
{
    public static class PascalCaseNameExtension
    {
        public static string ToPascalCase(this string name, CultureInfo culture = null)
        {
            culture = culture ?? CultureInfo.InvariantCulture;

            var builder = new StringBuilder(name.Length);
            var capitalizeNext = true;

            foreach (var c in name)
            {
                if (char.IsWhiteSpace(c) || c == '_')
                {
                    capitalizeNext = true;
                }
                else
                {
                    builder.Append(capitalizeNext ? char.ToUpper(c, culture) : char.ToLower(c, culture));
                    capitalizeNext = false;
                }
            }

            return builder.ToString();
        }
    }
}
