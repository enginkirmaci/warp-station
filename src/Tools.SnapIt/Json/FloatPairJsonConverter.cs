using System.Globalization;
using System.Text.Json;

namespace Tools.SnapIt.Json;

/// <summary>
/// Base class for converters that round-trip a pair of floats as the invariant
/// string "first,second" (e.g. <c>PointToStringJsonConverter</c> writes "10,20").
/// </summary>
public abstract class FloatPairJsonConverter<T> : JsonConverter<T>
{
    public override T? Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        string[] parts = reader.GetString().Split(',');
        T value = Create();
        SetFirst(value, float.Parse(parts[0], CultureInfo.InvariantCulture));
        SetSecond(value, float.Parse(parts[1], CultureInfo.InvariantCulture));
        return value;
    }

    public override void Write(Utf8JsonWriter writer, T value, JsonSerializerOptions options)
    {
        writer.WriteStringValue(FormattableString.Invariant($"{GetFirst(value)},{GetSecond(value)}"));
    }

    protected abstract T Create();

    protected abstract float GetFirst(T value);

    protected abstract float GetSecond(T value);

    protected abstract void SetFirst(T value, float first);

    protected abstract void SetSecond(T value, float second);
}
