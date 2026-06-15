using System.Globalization;
using System.Text.Json;
using Size = Tools.SnapIt.Graphics.Size;

namespace Tools.SnapIt.Json;

public class SizeToStringJsonConverter : JsonConverter<Size>
{
    public override Size? Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        string[] parts = reader.GetString().Split(',');
        return new Size
        {
            Width = float.Parse(parts[0], CultureInfo.InvariantCulture),
            Height = float.Parse(parts[1], CultureInfo.InvariantCulture)
        };
    }

    public override void Write(Utf8JsonWriter writer, Size value, JsonSerializerOptions options)
    {
        writer.WriteStringValue(FormattableString.Invariant($"{value.Width},{value.Height}"));
    }
}