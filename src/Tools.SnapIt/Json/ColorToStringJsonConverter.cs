using Avalonia.Media;

namespace Tools.SnapIt.Json;

public class ColorToStringJsonConverter : System.Text.Json.Serialization.JsonConverter<Color>
{
    public override Color Read(ref System.Text.Json.Utf8JsonReader reader, Type typeToConvert, System.Text.Json.JsonSerializerOptions options)
    {
        string hexString = reader.GetString();
        byte[] bytes = new byte[4];

        for (int i = 0; i < 4; i++)
        {
            bytes[i] = Convert.ToByte(hexString.Substring((i * 2) + 1, 2), 16);
        }

        return Color.FromArgb(bytes[0], bytes[1], bytes[2], bytes[3]);
    }

    public override void Write(System.Text.Json.Utf8JsonWriter writer, Color value, System.Text.Json.JsonSerializerOptions options)
    {
        string hexString = $"#{BitConverter.ToString([value.A, value.R, value.G, value.B]).Replace("-", "")}";
        writer.WriteStringValue(hexString);
    }
}
