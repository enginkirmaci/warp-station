using System.Text.Json;
using Tools.SnapIt.Common.Graphics;
using Point = Tools.SnapIt.Common.Graphics.Point;

namespace Tools.SnapIt.Common.Converters.Json;

public class PointToStringJsonConverter : JsonConverter<Point>
{
    public override Point? Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        string[] parts = reader.GetString().Split(',');
        return new Point
        {
            X = float.Parse(parts[0]),
            Y = float.Parse(parts[1])
        };
    }

    public override void Write(Utf8JsonWriter writer, Point value, JsonSerializerOptions options)
    {
        writer.WriteStringValue($"{value.X},{value.Y}");
    }
}