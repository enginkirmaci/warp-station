using System.Text.Json;
using System.Text.Json.Serialization;
using Avalonia;

namespace Tools.SnapIt.Json;

public class ThicknessToJsonConverter : JsonConverter<Thickness>
{
	public override Thickness Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
	{
		if (reader.TokenType == JsonTokenType.Number)
			return new Thickness(reader.GetDouble());

		if (reader.TokenType == JsonTokenType.String)
		{
			var str = reader.GetString();
			var parts = str.Split(',');
			if (parts.Length == 1)
				return new Thickness(double.Parse(parts[0].Trim()));
			if (parts.Length == 2)
				return new Thickness(double.Parse(parts[0].Trim()), double.Parse(parts[1].Trim()));
			if (parts.Length == 4)
				return new Thickness(
					double.Parse(parts[0].Trim()),
					double.Parse(parts[1].Trim()),
					double.Parse(parts[2].Trim()),
					double.Parse(parts[3].Trim()));
		}

		throw new JsonException($"Cannot convert '{reader.GetString()}' to Thickness.");
	}

	public override void Write(Utf8JsonWriter writer, Thickness value, JsonSerializerOptions options)
	{
		if (value.Left == value.Right && value.Top == value.Bottom && value.Left == value.Top)
		{
			writer.WriteNumberValue(value.Left);
		}
		else if (value.Left == value.Right && value.Top == value.Bottom)
		{
			writer.WriteStringValue($"{value.Left},{value.Top}");
		}
		else
		{
			writer.WriteStringValue($"{value.Left},{value.Top},{value.Right},{value.Bottom}");
		}
	}
}
