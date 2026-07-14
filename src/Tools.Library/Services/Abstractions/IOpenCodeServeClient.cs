using Tools.Library.Services.OpenCode.Serve;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// HTTP/SSE client for a running <c>opencode serve</c> instance. This is the only place
/// that knows the real endpoint paths and JSON shapes; the higher-level
/// <see cref="IOpenCodeServeService"/> consumes these typed results.
/// </summary>
public interface IOpenCodeServeClient
{
    /// <summary>
    /// Configures the client to talk to the server at <paramref name="baseUrl"/> (e.g.
    /// <c>http://127.0.0.1:4096</c>), applying basic auth when <paramref name="authToken"/>
    /// is non-empty. Safe to call again when the connection settings change.
    /// </summary>
    void Configure(string baseUrl, string? authToken);

    /// <summary>
    /// Returns <see langword="true"/> when <c>GET /global/health</c> reports a healthy server.
    /// Never throws — a failed/unreachable server yields <see langword="false"/>.
    /// </summary>
    Task<bool> PingAsync(CancellationToken cancellationToken = default);

    /// <summary>
    /// Lists serve sessions (<c>GET /session</c>), optionally scoped to a working directory
    /// via the <c>?directory=</c> query. Never throws on transport failure; returns an empty
    /// list and logs.
    /// </summary>
    Task<IReadOnlyList<ServeSession>> ListSessionsAsync(string? directory = null, CancellationToken cancellationToken = default);

    /// <summary>
    /// Streams the global event bus (<c>GET /global/event</c>), yielding one parsed
    /// <see cref="ServeGlobalEvent"/> per SSE message. The enumeration completes if the
    /// connection drops or the <paramref name="cancellationToken"/> is cancelled.
    /// </summary>
    IAsyncEnumerable<ServeGlobalEvent> StreamGlobalEventsAsync(CancellationToken cancellationToken);

    /// <summary>
    /// Replies to a pending permission request (<c>POST /session/{id}/permissions/{permissionID}</c>).
    /// Pass <see cref="ServePermissionResponse.Once"/> to approve this one request,
    /// <see cref="ServePermissionResponse.Always"/> to approve and remember, or
    /// <see cref="ServePermissionResponse.Reject"/> to deny. Returns <see langword="true"/> on
    /// HTTP 200; <see langword="false"/> (and logs) on failure.
    /// </summary>
    Task<bool> ReplyPermissionAsync(string sessionId, string permissionId, ServePermissionResponse response, CancellationToken cancellationToken = default);
}
