using System.Text.Json;
using System.Text.Json.Serialization;

namespace Tools.Library.Services.OpenCode.Serve;

/// <summary>
/// A serve session, as returned by <c>GET /session</c>. Only the fields the integration
/// needs are mapped; unknown JSON fields are ignored. Property names use the wire casing
/// (<see cref="JsonPropertyNameAttribute"/>).
/// </summary>
public sealed class ServeSession
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    /// <summary>The working directory the session runs in — used to match a launched instance to its repo folder.</summary>
    [JsonPropertyName("directory")]
    public string Directory { get; set; } = string.Empty;

    [JsonPropertyName("title")]
    public string Title { get; set; } = string.Empty;

    [JsonPropertyName("projectID")]
    public string? ProjectId { get; set; }
}

/// <summary>
/// The response sent to <c>POST /session/{id}/permissions/{permissionID}</c>.
/// </summary>
[JsonConverter(typeof(ServePermissionResponseConverter))]
public enum ServePermissionResponse
{
    /// <summary>Approve this one request.</summary>
    Once,

    /// <summary>Approve and remember (auto-approve future matching requests this session).</summary>
    Always,

    /// <summary>Deny the request.</summary>
    Reject,
}

internal sealed class ServePermissionResponseConverter : JsonConverter<ServePermissionResponse>
{
    public override ServePermissionResponse Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
        => reader.GetString() switch
        {
            "once" => ServePermissionResponse.Once,
            "always" => ServePermissionResponse.Always,
            "reject" => ServePermissionResponse.Reject,
            _ => ServePermissionResponse.Once,
        };

    public override void Write(Utf8JsonWriter writer, ServePermissionResponse value, JsonSerializerOptions options)
        => writer.WriteStringValue(value switch
        {
            ServePermissionResponse.Once => "once",
            ServePermissionResponse.Always => "always",
            ServePermissionResponse.Reject => "reject",
            _ => "once",
        });
}

/// <summary>
/// One event decoded from the <c>GET /global/event</c> SSE stream. The stream wraps each bus
/// event with the directory it originated from; the payload is kept as a raw JSON element so
/// callers can pull typed fields by event type without modeling the full (large) event union.
/// </summary>
public sealed class ServeGlobalEvent
{
    /// <summary>The working directory the event originated from.</summary>
    [JsonPropertyName("directory")]
    public string Directory { get; set; } = string.Empty;

    /// <summary>The raw event payload, including its <c>type</c> discriminator and <c>properties</c>.</summary>
    [JsonPropertyName("payload")]
    public JsonElement Payload { get; set; }

    /// <summary>The event <c>type</c> string (e.g. <c>permission.updated</c>), or empty when absent.</summary>
    public string Type => Payload.ValueKind == JsonValueKind.Object
        && Payload.TryGetProperty("type", out var t)
        && t.ValueKind == JsonValueKind.String
            ? t.GetString() ?? string.Empty
            : string.Empty;
}

/// <summary>
/// The response shape of <c>GET /config/providers</c>: the configured providers (each with
/// its own model map) plus the per-category default model ids. Only the fields the
/// integration needs are mapped; unknown JSON fields are ignored.
/// </summary>
public sealed class ServeProvidersResponse
{
    [JsonPropertyName("providers")]
    public List<ServeProvider> Providers { get; set; } = new();

    /// <summary>
    /// Maps a category (e.g. <c>"model"</c>, <c>"small_model"</c>) to the default model id for
    /// that category. Values are plain model ids; the <c>provider/</c> prefix may or may not be
    /// present, so matching against the flattened list tolerates both forms.
    /// </summary>
    [JsonPropertyName("default")]
    public Dictionary<string, string> Default { get; set; } = new();
}

/// <summary>
/// One provider as returned by <c>GET /config/providers</c>. <see cref="Models"/> maps each
/// model id (provider-local) to its info; the flattened selector id is
/// <c>{Id}/{modelId}</c>.
/// </summary>
public sealed class ServeProvider
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    [JsonPropertyName("models")]
    public Dictionary<string, ServeProviderModel> Models { get; set; } = new();
}

/// <summary>
/// One model within a provider. Only the display <see cref="Name"/> is captured; the
/// flattened selector uses the <c>provider/model-id</c> key, not this name.
/// </summary>
public sealed class ServeProviderModel
{
    [JsonPropertyName("name")]
    public string? Name { get; set; }
}

/// <summary>
/// The properties of a <c>permission.updated</c> event: a tool action awaiting approval.
/// Parsed lazily from <see cref="ServeGlobalEvent.Payload"/> by the client.
/// </summary>
public sealed class ServePermission
{
    [JsonPropertyName("id")]
    public string Id { get; set; } = string.Empty;

    [JsonPropertyName("type")]
    public string Type { get; set; } = string.Empty;

    [JsonPropertyName("sessionID")]
    public string SessionId { get; set; } = string.Empty;

    [JsonPropertyName("messageID")]
    public string MessageId { get; set; } = string.Empty;

    [JsonPropertyName("callID")]
    public string? CallId { get; set; }

    [JsonPropertyName("title")]
    public string Title { get; set; } = string.Empty;
}
