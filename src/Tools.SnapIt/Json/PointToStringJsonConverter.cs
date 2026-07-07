using System.Globalization;
using System.Text.Json;
using Point = Tools.SnapIt.Graphics.Point;

namespace Tools.SnapIt.Json;

public class PointToStringJsonConverter : JsonConverter<Point>
{
    public override Point? Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        string[] parts = reader.GetString().Split(',');
        return new Point
        {
            X = float.Parse(parts[0], CultureInfo.InvariantCulture),
            Y = float.Parse(parts[1], CultureInfo.InvariantCulture)
        };
    }

    public override void Write(Utf8JsonWriter writer, Point value, JsonSerializerOptions options)
    {
        writer.WriteStringValue(FormattableString.Invariant($"{value.X},{value.Y}"));
    }
}