using System.Text.Json;
using Tools.SnapIt.Entities;

namespace Tools.SnapIt.Helpers;

public static class Json
{
	public static T? Deserialize<T>(string value)
	{
		try
		{
			return JsonSerializer.Deserialize<T>(value, JsonOptions.DefaultOptions);
		}
		catch
		{
			return default;
		}
	}

	public static string? Serialize<T>(T value)
	{
		try
		{
			return JsonSerializer.Serialize(value, JsonOptions.DefaultOptions);
		}
		catch
		{
			return default;
		}
	}
}