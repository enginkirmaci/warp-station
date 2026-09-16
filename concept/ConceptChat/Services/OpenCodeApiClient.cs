using System.Net.Http;
using System.Runtime.CompilerServices;
using System.Text;
using System.Text.Json;

namespace ConceptChat.Services;

/// <summary>
/// REST + SSE calls against an opencode server (v1 surface verified live against
/// 1.18.31). The chat is driven purely by the /event stream: the message POST is
/// fire-and-forget, assistant output arrives as message.part.updated events whose
/// "part" carries the full text-so-far each time.
/// </summary>
public sealed class OpenCodeApiClient(string baseUrl)
{
    private static readonly HttpClient Http = new() { Timeout = Timeout.InfiniteTimeSpan };
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public string BaseUrl { get; } = baseUrl.TrimEnd('/');

    public async Task<string> CreateSessionAsync(string title, CancellationToken ct)
    {
        using var resp = await PostAsync("/session", new { title }, TimeSpan.FromSeconds(30), ct);
        using var doc = JsonDocument.Parse(await resp.Content.ReadAsStringAsync(ct));
        return doc.RootElement.GetProperty("id").GetString()
               ?? throw new InvalidOperationException("session response is missing id");
    }

    public Task SendMessageAsync(string sessionId, string text, string? model, CancellationToken ct)
    {
        object body = model is null
            ? new { parts = new object[] { new { type = "text", text } } }
            : new
            {
                parts = new object[] { new { type = "text", text } },
                model = SplitModel(model),
            };
        return PostAsync($"/session/{sessionId}/message", body, Timeout.InfiniteTimeSpan, ct);
    }

    public Task RespondPermissionAsync(string sessionId, string permissionId, string response, CancellationToken ct) =>
        PostAsync($"/session/{sessionId}/permissions/{permissionId}", new { response }, TimeSpan.FromSeconds(15), ct);

    public Task ReplyV2Async(string requestId, string reply, CancellationToken ct) =>
        PostAsync($"/permission/{requestId}/reply", new { reply }, TimeSpan.FromSeconds(15), ct);

    public async Task<List<string>> GetModelsAsync(CancellationToken ct)
    {
        using var resp = await Http.GetAsync($"{BaseUrl}/config/providers", ct);
        resp.EnsureSuccessStatusCode();
        using var doc = JsonDocument.Parse(await resp.Content.ReadAsStringAsync(ct));
        var models = new List<string>();
        if (doc.RootElement.TryGetProperty("providers", out var providers) && providers.ValueKind == JsonValueKind.Array)
        {
            foreach (var provider in providers.EnumerateArray())
            {
                var pid = provider.TryGetProperty("id", out var pi) ? pi.GetString() : null;
                if (pid is null || !provider.TryGetProperty("models", out var modelsMap) ||
                    modelsMap.ValueKind != JsonValueKind.Object)
                    continue;
                models.AddRange(modelsMap.EnumerateObject().Select(m => $"{pid}/{m.Name}"));
            }
        }
        return models;
    }

    public async IAsyncEnumerable<JsonElement> StreamEventsAsync([EnumeratorCancellation] CancellationToken ct)
    {
        using var req = new HttpRequestMessage(HttpMethod.Get, $"{BaseUrl}/event");
        using var resp = await Http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, ct);
        resp.EnsureSuccessStatusCode();
        await using var stream = await resp.Content.ReadAsStreamAsync(ct);
        var reader = new StreamReader(stream);

        while (await reader.ReadLineAsync(ct) is { } line)
        {
            if (!line.StartsWith("data:", StringComparison.Ordinal))
                continue;
            var payload = line["data:".Length..].Trim();
            if (payload.Length == 0)
                continue;
            JsonElement evt;
            try
            {
                using var doc = JsonDocument.Parse(payload);
                evt = doc.RootElement.Clone();
            }
            catch (JsonException)
            {
                continue;
            }
            yield return evt;
        }
    }

    // a ValueTuple would serialize as an ARRAY; the API needs the object shape
    private static object SplitModel(string model)
    {
        var split = model.Split('/', 2);
        return split.Length == 2
            ? new { providerID = split[0], modelID = split[1] }
            : new { providerID = model, modelID = "" };
    }

    private async Task<HttpResponseMessage> PostAsync(string path, object body, TimeSpan timeout, CancellationToken ct)
    {
        using var timeoutCts = timeout == Timeout.InfiniteTimeSpan
            ? null
            : CancellationTokenSource.CreateLinkedTokenSource(ct);
        timeoutCts?.CancelAfter(timeout);
        using var content = new StringContent(JsonSerializer.Serialize(body, JsonOptions), Encoding.UTF8, "application/json");
        var resp = await Http.PostAsync(BaseUrl + path, content, timeoutCts?.Token ?? ct);
        if (!resp.IsSuccessStatusCode)
        {
            var detail = Truncate(await resp.Content.ReadAsStringAsync(ct), 300);
            throw new HttpRequestException($"{(int)resp.StatusCode} {resp.ReasonPhrase}{(detail.Length > 0 ? $" — {detail}" : "")}");
        }
        return resp;
    }

    private static string Truncate(string s, int max) => s.Length <= max ? s : s[..max] + "…";
}
