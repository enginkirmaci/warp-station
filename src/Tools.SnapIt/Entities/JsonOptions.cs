using System.Text.Json;
using Tools.SnapIt.Json;

namespace Tools.SnapIt.Entities;

public class JsonOptions
{
	private static readonly Lazy<JsonSerializerOptions> defaultOptions = new(() =>
	{
		var options = new JsonSerializerOptions
		{
			DictionaryKeyPolicy = JsonNamingPolicy.CamelCase,
			DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
			IgnoreReadOnlyProperties = true,
			PropertyNameCaseInsensitive = true,
			ReadCommentHandling = JsonCommentHandling.Skip,
			WriteIndented = true,
			ReferenceHandler = ReferenceHandler.IgnoreCycles,
			NumberHandling = JsonNumberHandling.AllowReadingFromString | JsonNumberHandling.WriteAsString
		};

		options.Converters.Add(new JsonStringEnumConverter());
		options.Converters.Add(new SizeToStringJsonConverter());
		options.Converters.Add(new PointToStringJsonConverter());
		options.Converters.Add(new ColorToStringJsonConverter());

		return options;
	});

	public static JsonSerializerOptions DefaultOptions => defaultOptions.Value;
}