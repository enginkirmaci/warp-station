using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using Serilog;
using Tools.Library.Services.Abstractions;
using Tools.Library.Services.OpenCode.Serve;

namespace Tools.Library.Services.OpenCode;

/// <summary>
/// Default <see cref="IOpenCodeServeClient"/>. A thin <see cref="HttpClient"/>-based wrapper
/// around the <c>opencode serve</c> HTTP API (health, sessions, permission replies) plus a
/// hand-rolled SSE reader for <c>GET /global/event</c>. All transport failures are caught and
/// logged — the client never throws for unreachable-server or malformed-payload conditions;
/// callers get empty/false results instead, so the UI can show "Disconnected" gracefully.
/// </summary>
public sealed class OpenCodeServeClient : IOpenCodeServeClient
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        // Tolerate extra/unknown fields (the serve schema is large and evolves).
        UnmappedMemberHandling = System.Text.Json.Serialization.JsonUnmappedMemberHandling.Skip,
    };

    // Endpoints (see https://opencode.ai/docs/server/). Kept as constants so the whole API
    // surface lives in this one file.
    private const string HealthPath = "/global/health";
    private const string SessionsPath = "/session";
    private const string GlobalEventPath = "/global/event";
    private const string ProvidersPath = "/config/providers";

    private readonly HttpClient _http;
    private string _baseUrl = string.Empty;
    private string? _authToken;

    public OpenCodeServeClient()
    {
        _http = new HttpClient { Timeout = TimeSpan.FromSeconds(10) };
    }

    /// <inheritdoc/>
    public void Configure(string baseUrl, string? authToken)
    {
        _baseUrl = (baseUrl ?? string.Empty).TrimEnd('/');
        _authToken = string.IsNullOrWhiteSpace(authToken) ? null : authToken;
    }

    /// <inheritdoc/>
    public async Task<bool> PingAsync(CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(_baseUrl)) return false;
        try
        {
            using var req = BuildRequest(HttpMethod.Get, HealthPath);
            using var resp = await _http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, cancellationToken);
            if (!resp.IsSuccessStatusCode) return false;
            await using var stream = await resp.Content.ReadAsStreamAsync(cancellationToken);
            using var doc = await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken);
            return doc.RootElement.TryGetProperty("healthy", out var h) && h.GetBoolean();
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "OpenCodeServeClient: ping failed");
            return false;
        }
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<ServeSession>> ListSessionsAsync(string? directory = null, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(_baseUrl)) return Array.Empty<ServeSession>();
        try
        {
            var path = SessionsPath;
            if (!string.IsNullOrWhiteSpace(directory))
                path += "?directory=" + Uri.EscapeDataString(directory);

            using var req = BuildRequest(HttpMethod.Get, path);
            using var resp = await _http.SendAsync(req, cancellationToken);
            if (!resp.IsSuccessStatusCode) return Array.Empty<ServeSession>();

            var sessions = await resp.Content.ReadFromJsonAsync<List<ServeSession>>(JsonOptions, cancellationToken);
            return sessions ?? new List<ServeSession>();
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "OpenCodeServeClient: list sessions failed");
            return Array.Empty<ServeSession>();
        }
    }

    /// <inheritdoc/>
    public async Task<ServeProvidersResponse> ListProvidersAsync(string? directory = null, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(_baseUrl)) return new ServeProvidersResponse();
        try
        {
            var path = ProvidersPath;
            if (!string.IsNullOrWhiteSpace(directory))
                path += "?directory=" + Uri.EscapeDataString(directory);

            using var req = BuildRequest(HttpMethod.Get, path);
            using var resp = await _http.SendAsync(req, cancellationToken);
            if (!resp.IsSuccessStatusCode) return new ServeProvidersResponse();

            return await resp.Content.ReadFromJsonAsync<ServeProvidersResponse>(JsonOptions, cancellationToken)
                ?? new ServeProvidersResponse();
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "OpenCodeServeClient: list providers failed");
            return new ServeProvidersResponse();
        }
    }

    /// <inheritdoc/>
    public async IAsyncEnumerable<ServeGlobalEvent> StreamGlobalEventsAsync(
        [System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken cancellationToken)
    {
        if (string.IsNullOrEmpty(_baseUrl)) yield break;

        // Open the SSE connection (may throw if serve is down); failures end the enumeration
        // quietly. Done as a separate step because a `yield` cannot live inside a try/catch.
        var opened = await OpenEventStreamAsync(cancellationToken);
        if (opened is null) yield break;

        var (resp, stream, cts) = opened.Value;
        try
        {
            await foreach (var evt in ReadSseStreamAsync(stream, cts.Token).WithCancellation(cts.Token))
                yield return evt;
        }
        finally
        {
            await TryDisposeAsync(stream);
            resp.Dispose();
            cts.Dispose();
        }
    }

    /// <summary>
    /// Opens the global event stream, returning the live response/stream plus a linked
    /// cancellation source, or <see langword="null"/> if the connection could not be opened.
    /// Logs transport failures rather than throwing.
    /// </summary>
    private async Task<(HttpResponseMessage Resp, Stream Stream, CancellationTokenSource Cts)?> OpenEventStreamAsync(CancellationToken cancellationToken)
    {
        try
        {
            var req = BuildRequest(HttpMethod.Get, GlobalEventPath);
            req.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("text/event-stream"));
            var cts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            var resp = await _http.SendAsync(req, HttpCompletionOption.ResponseHeadersRead, cts.Token);
            resp.EnsureSuccessStatusCode();
            var stream = await resp.Content.ReadAsStreamAsync(cts.Token);
            return (resp, stream, cts);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            // The stream dropping (serve stopped/restarted) ends the enumeration quietly.
            Log.Logger.Debug(ex, "OpenCodeServeClient: event stream ended");
            return null;
        }
    }

    /// <inheritdoc/>
    public async Task<bool> ReplyPermissionAsync(string sessionId, string permissionId, ServePermissionResponse response, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(_baseUrl)) return false;
        try
        {
            var path = $"/session/{Uri.EscapeDataString(sessionId)}/permissions/{Uri.EscapeDataString(permissionId)}";
            using var req = BuildRequest(HttpMethod.Post, path);
            req.Content = JsonContent.Create(new { response }, options: JsonOptions);

            using var resp = await _http.SendAsync(req, cancellationToken);
            if (!resp.IsSuccessStatusCode)
            {
                Log.Logger.Warning("OpenCodeServeClient: permission reply {Response} for {PermissionId} returned {Status}", response, permissionId, (int)resp.StatusCode);
                return false;
            }
            return true;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Warning(ex, "OpenCodeServeClient: permission reply failed");
            return false;
        }
    }

    // --- helpers ---

    private HttpRequestMessage BuildRequest(HttpMethod method, string path)
    {
        var req = new HttpRequestMessage(method, _baseUrl + path);
        if (_authToken is not null)
        {
            // opencode serve expects HTTP basic auth with username "opencode".
            var token = Convert.ToBase64String(Encoding.UTF8.GetBytes("opencode:" + _authToken));
            req.Headers.Authorization = new AuthenticationHeaderValue("Basic", token);
        }
        return req;
    }

    /// <summary>
    /// Minimal SSE reader: groups consecutive <c>data:</c> lines into one event payload, then
    /// parses the accumulated JSON as a <see cref="ServeGlobalEvent"/>. Handles multi-line
    /// <c>data:</c> fields and ignores comments/keep-alive blank lines per the SSE spec.
    /// </summary>
    private static async IAsyncEnumerable<ServeGlobalEvent> ReadSseStreamAsync(
        Stream stream,
        [System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken cancellationToken)
    {
        using var reader = new StreamReader(stream);
        var data = new StringBuilder();

        while (!cancellationToken.IsCancellationRequested)
        {
            var line = await reader.ReadLineAsync(cancellationToken);
            if (line is null) yield break; // end of stream

            if (line.Length == 0)
            {
                // Blank line dispatches the accumulated event.
                if (data.Length > 0)
                {
                    var evt = TryParse(data.ToString());
                    if (evt is not null) yield return evt;
                    data.Clear();
                }
                continue;
            }

            if (line.StartsWith("data:", StringComparison.Ordinal))
            {
                // Per the SSE spec, strip a single leading space after "data:" if present.
                var value = line.AsSpan(5);
                if (value.Length > 0 && value[0] == ' ')
                    value = value[1..];
                if (data.Length > 0) data.Append('\n');
                data.Append(value);
            }
            // Ignore id:/event:/retry:/comment (':') lines — we only need data.
        }
    }

    private static ServeGlobalEvent? TryParse(string json)
    {
        try
        {
            json = json.Trim();
            if (json.Length == 0 || json == "[DONE]") return null;
            return JsonSerializer.Deserialize<ServeGlobalEvent>(json, JsonOptions);
        }
        catch (Exception ex)
        {
            Log.Logger.Debug(ex, "OpenCodeServeClient: failed to parse SSE data chunk");
            return null;
        }
    }

    private static async Task TryDisposeAsync(IDisposable? disposable)
    {
        if (disposable is IAsyncDisposable ad)
            await ad.DisposeAsync();
        else
            disposable?.Dispose();
    }
}
