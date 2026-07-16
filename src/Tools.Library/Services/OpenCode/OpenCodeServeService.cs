using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using Tools.Library.Services.OpenCode.Serve;

namespace Tools.Library.Services.OpenCode;

/// <summary>
/// Default <see cref="IOpenCodeServeService"/>. Manages a single global <c>opencode serve</c>
/// subprocess and a registry of per-repo instances. Runs two background loops while started:
/// <list type="bullet">
/// <item>A health poll (every few seconds) toggling <see cref="IsConnected"/>.</item>
/// <item>The global SSE listener that matches sessions to registered instances and
/// auto-approves their <c>permission.updated</c> events when the instance has
/// <see cref="OpenCodeInstance.AutoApprove"/> set.</item>
/// </list>
/// </summary>
public sealed class OpenCodeServeService : IOpenCodeServeService
{
    // --- timing ---
    private static readonly TimeSpan HealthInterval = TimeSpan.FromSeconds(5);
    private static readonly TimeSpan InitialHealthTimeout = TimeSpan.FromSeconds(8);

    /// <summary>
    /// How long to wait after launch before transitioning an unmatched instance from
    /// <see cref="OpenCodeInstanceStatus.Starting"/> to <see cref="OpenCodeInstanceStatus.Running"/>.
    /// The terminal-launched TUI may never register a session with our serve instance, so after
    /// this grace we trust the launch succeeded.
    /// </summary>
    private static readonly TimeSpan StartingGracePeriod = TimeSpan.FromSeconds(15);

    // --- collaborators ---
    private readonly IOpenCodeServeClient _client;
    private readonly IRepoService _repoService;

    // --- state ---
    private OpenCodeServeSettings _settings = new();
    private Process? _process;
    private bool _started;
    private bool _disposed;

    private readonly CancellationTokenSource _shutdownCts = new();
    private readonly ConcurrentDictionary<string, OpenCodeInstance> _instances = new(StringComparer.OrdinalIgnoreCase);

    public bool IsConnected { get; private set; }

    public event EventHandler<bool>? ConnectionChanged;
    public event Action<OpenCodeInstance>? InstanceChanged;

    public OpenCodeServeService(IOpenCodeServeClient client, IRepoService repoService)
    {
        _client = client;
        _repoService = repoService;
    }

    /// <inheritdoc/>
    public async Task EnsureStartedAsync(OpenCodeServeSettings settings, bool force = false, CancellationToken cancellationToken = default)
    {
        _settings = settings ?? new OpenCodeServeSettings();
        if (!force && !_settings.AutoConnect) return;
        if (_started) return;

        _started = true;
        var baseUrl = BuildBaseUrl(_settings);
        _client.Configure(baseUrl, _settings.AuthToken);

        StartSubprocess();
        // Kick off the background loops (health poll + event listener). They own their own
        // lifetimes against _shutdownCts; we don't await them here.
        _ = Task.Run(() => HealthLoopAsync(_shutdownCts.Token));
        _ = Task.Run(() => EventLoopAsync(_shutdownCts.Token));

        // Give the server a short window to come up so auto-connect reflects reality quickly.
        await WaitInitialConnectAsync(cancellationToken);
    }

    /// <inheritdoc/>
    public void Stop()
    {
        if (_disposed) return;
        _disposed = true;
        _shutdownCts.Cancel();

        foreach (var instance in _instances.Values)
            instance.Status = OpenCodeInstanceStatus.Stopped;

        try { _process?.Kill(entireProcessTree: true); } catch { }
        try { _process?.Dispose(); } catch { }
        _process = null;
        _started = false;
        SetConnected(false);
    }

    /// <inheritdoc/>
    public OpenCodeInstance Register(string folderPath, int? pid, bool autoApprove)
    {
        if (string.IsNullOrWhiteSpace(folderPath))
            throw new ArgumentException("folderPath is required", nameof(folderPath));

        var instance = new OpenCodeInstance
        {
            FolderPath = folderPath,
            Pid = pid,
            AutoApprove = autoApprove,
            Status = OpenCodeInstanceStatus.Starting,
            StartedAt = DateTimeOffset.UtcNow,
        };
        instance.StatusChanged += OnInstanceStatusChanged;
        instance.AutoApproveChanged += OnInstanceAutoApproveChanged;

        // Replace any prior entry for this folder (a re-launch supersedes a stopped one).
        if (_instances.TryGetValue(folderPath, out var existing))
        {
            existing.StatusChanged -= OnInstanceStatusChanged;
            existing.AutoApproveChanged -= OnInstanceAutoApproveChanged;
        }
        _instances[folderPath] = instance;

        // Mirror onto the matching repo immediately so the card reflects the launch.
        ApplyToRepo(instance);
        InstanceChanged?.Invoke(instance);
        return instance;
    }

    /// <inheritdoc/>
    public void Unregister(string folderPath)
    {
        if (_instances.TryRemove(folderPath, out var instance))
        {
            instance.StatusChanged -= OnInstanceStatusChanged;
            instance.AutoApproveChanged -= OnInstanceAutoApproveChanged;
            ClearRepoStatus(folderPath);
        }
    }

    /// <inheritdoc/>
    public OpenCodeInstance? GetInstance(string folderPath)
        => _instances.TryGetValue(folderPath, out var i) ? i : null;

    /// <inheritdoc/>
    public void SetAutoApprove(string folderPath, bool value)
    {
        if (_instances.TryGetValue(folderPath, out var instance))
            instance.AutoApprove = value;
    }

    /// <inheritdoc/>
    public async Task<ServeModelsResult> GetModelsAsync(CancellationToken cancellationToken = default)
    {
        // No live serve to query (not started / down) — the selector shows its hint instead.
        if (!IsConnected) return ServeModelsResult.Empty;

        ServeProvidersResponse response;
        try { response = await _client.ListProvidersAsync(null, cancellationToken); }
        catch (Exception ex)
        {
            Log.Logger.Debug(ex, "OpenCodeServeService: list providers failed");
            return ServeModelsResult.Empty;
        }

        var providers = response.Providers;
        if (providers.Count == 0) return ServeModelsResult.Empty;

        // Flatten each provider's model map to opencode's `provider/model-id` selector id,
        // skipping empty provider/model ids. Sorted by provider then model for stable order.
        var models = new List<string>();
        foreach (var provider in providers.OrderBy(p => p.Id, StringComparer.OrdinalIgnoreCase))
        {
            if (string.IsNullOrWhiteSpace(provider.Id)) continue;
            foreach (var modelId in provider.Models.Keys.OrderBy(k => k, StringComparer.OrdinalIgnoreCase))
            {
                if (string.IsNullOrWhiteSpace(modelId)) continue;
                models.Add($"{provider.Id}/{modelId}");
            }
        }

        if (models.Count == 0) return ServeModelsResult.Empty;

        // The default map is keyed by category (e.g. "model", "small_model"); prefer the
        // top-level "model" default, then any value, matching it against the flattened ids
        // (the response may use either the bare id or the `provider/id` form).
        var modelSet = new HashSet<string>(models, StringComparer.Ordinal);
        string? defaultModel = null;
        if (response.Default.TryGetValue("model", out var primary) && Matches(primary, modelSet, out var hit))
            defaultModel = hit;
        else
        {
            foreach (var value in response.Default.Values)
            {
                if (Matches(value, modelSet, out hit)) { defaultModel = hit; break; }
            }
        }

        return new ServeModelsResult(models, defaultModel ?? models[0]);
    }

    /// <summary>
    /// Returns <see langword="true"/> when <paramref name="value"/> (a default-model id) names
    /// a model in <paramref name="models"/>, tolerating both the bare id and the
    /// <c>provider/id</c> forms, with the matched canonical id in <paramref name="match"/>.
    /// </summary>
    private static bool Matches(string? value, HashSet<string> models, out string match)
    {
        match = string.Empty;
        if (string.IsNullOrWhiteSpace(value)) return false;
        if (models.Contains(value!)) { match = value!; return true; }
        // Bare id -> match the suffix after the last '/' of every flattened id.
        var bare = value.AsSpan(value.LastIndexOf('/') + 1);
        foreach (var m in models)
        {
            var suffix = m.AsSpan(m.LastIndexOf('/') + 1);
            if (suffix.Equals(bare, StringComparison.OrdinalIgnoreCase))
            {
                match = m;
                return true;
            }
        }
        return false;
    }

    // --- background loops ---

    /// <summary>
    /// Polls <c>/global/health</c> and toggles <see cref="IsConnected"/>. While connected,
    /// also periodically re-syncs session→instance matches so a newly-started instance is
    /// discovered even without an intervening event.
    /// </summary>
    private async Task HealthLoopAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            var healthy = await _client.PingAsync(ct);
            SetConnected(healthy);

            if (healthy)
            {
                try { await SyncSessionsAsync(ct); }
                catch (Exception ex) { Log.Logger.Debug(ex, "OpenCodeServeService: session sync failed"); }
            }

            try { await Task.Delay(HealthInterval, ct); }
            catch (OperationCanceledException) { break; }
        }
    }

    /// <summary>
    /// (Re)opens the global event stream. Exits when cancelled; the loop will reopen it on the
    /// next health tick by being restarted — but since the stream blocks, we run it directly
    /// and reconnect here after a drop.
    /// </summary>
    private async Task EventLoopAsync(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            if (!IsConnected)
            {
                try { await Task.Delay(HealthInterval, ct); } catch (OperationCanceledException) { break; }
                continue;
            }

            try
            {
                await foreach (var evt in _client.StreamGlobalEventsAsync(ct).WithCancellation(ct))
                    HandleEvent(evt);
            }
            catch (OperationCanceledException) { break; }
            catch (Exception ex)
            {
                Log.Logger.Debug(ex, "OpenCodeServeService: event stream dropped, will reconnect");
                try { await Task.Delay(HealthInterval, ct); } catch (OperationCanceledException) { break; }
            }
        }
    }

    private async Task WaitInitialConnectAsync(CancellationToken ct)
    {
        using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        cts.CancelAfter(InitialHealthTimeout);
        try
        {
            while (!cts.IsCancellationRequested)
            {
                if (await _client.PingAsync(cts.Token))
                {
                    SetConnected(true);
                    return;
                }
                await Task.Delay(TimeSpan.FromMilliseconds(500), cts.Token);
            }
        }
        catch (OperationCanceledException) { }
    }

    // --- event handling ---

    private void HandleEvent(ServeGlobalEvent evt)
    {
        if (string.IsNullOrEmpty(evt.Type)) return;

        if (evt.Type == "permission.updated")
        {
            HandlePermissionUpdated(evt);
            return;
        }

        // session.status / session.idle / session.updated let us keep instance status fresh.
        if (evt.Type.StartsWith("session.", StringComparison.Ordinal))
            _ = SyncSessionsAsync(_shutdownCts.Token);
    }

    /// <summary>
    /// Auto-approves a <c>permission.updated</c> when its session belongs to a registered
    /// instance with auto-approve enabled. Session→instance matching is by working directory
    /// (the event carries <c>directory</c>).
    /// </summary>
    private void HandlePermissionUpdated(ServeGlobalEvent evt)
    {
        ServePermission? perm;
        try
        {
            perm = evt.Payload.TryGetProperty("properties", out var props)
                ? props.Deserialize<ServePermission>()
                : null;
        }
        catch (Exception ex)
        {
            Log.Logger.Debug(ex, "OpenCodeServeService: failed to parse permission.updated payload");
            return;
        }

        if (perm is null || string.IsNullOrEmpty(perm.SessionId)) return;

        // Match by the event directory (preferred) then by session id as a fallback.
        var dir = NormalizePath(evt.Directory);
        var instance = FindInstanceByDirectory(dir) ?? FindInstanceBySession(perm.SessionId);
        if (instance is null || !instance.AutoApprove) return;

        // Fire-and-forget the reply; the client logs failures and never throws.
        _ = _client.ReplyPermissionAsync(perm.SessionId, perm.Id, ServePermissionResponse.Once, _shutdownCts.Token)
            .ContinueWith(t =>
            {
                if (t.IsCompletedSuccessfully && t.Result)
                    Log.Logger.Information("OpenCodeServeService: auto-approved '{Title}' for {Folder}", perm.Title, instance.FolderPath);
            }, TaskScheduler.Default);
    }

    // --- session ↔ instance matching ---

    /// <summary>
    /// Lists serve sessions and reconciles them against registered instances. The terminal-
    /// launched opencode TUI is a separate process from <c>opencode serve</c> and may not
    /// register sessions visible to our serve instance, so session matching is used only to
    /// <em>upgrade</em> status (Starting→Running) and acquire a SessionId for auto-approve —
    /// never to <em>downgrade</em> to Stopped when no session is found. A previously-matched
    /// session disappearing IS treated as stopped (the user closed opencode).
    /// </summary>
    private async Task SyncSessionsAsync(CancellationToken ct)
    {
        if (_instances.IsEmpty) return;

        IReadOnlyList<ServeSession> sessions;
        try { sessions = await _client.ListSessionsAsync(null, ct); }
        catch { return; }

        var byDir = new Dictionary<string, ServeSession>(StringComparer.OrdinalIgnoreCase);
        foreach (var s in sessions)
        {
            if (!string.IsNullOrEmpty(s.Directory))
                byDir[NormalizePath(s.Directory)] = s;
        }

        foreach (var instance in _instances.Values)
        {
            var key = NormalizePath(instance.FolderPath);
            if (byDir.TryGetValue(key, out var session))
            {
                // Found a live session for this folder — upgrade to Running and remember the id.
                instance.SessionId = session.Id;
                instance.Status = OpenCodeInstanceStatus.Running;
            }
            else if (instance.SessionId is not null
                     && sessions.Any(s => string.Equals(s.Id, instance.SessionId, StringComparison.Ordinal)))
            {
                // The previously-matched session is still alive.
                instance.Status = OpenCodeInstanceStatus.Running;
            }
            else if (instance.SessionId is not null)
            {
                // We HAD a matched session before, and now it's gone — the user closed opencode.
                instance.Status = OpenCodeInstanceStatus.Stopped;
            }
            else
            {
                // No session was ever found. The terminal-launched TUI is a separate process
                // from serve and may simply not register sessions here. Trust the launch: after
                // a brief grace, transition Starting→Running so the UI reflects reality.
                if (instance.Status == OpenCodeInstanceStatus.Starting)
                {
                    var age = instance.StartedAt is null
                        ? TimeSpan.MaxValue
                        : DateTimeOffset.UtcNow - instance.StartedAt.Value;
                    if (age > StartingGracePeriod)
                        instance.Status = OpenCodeInstanceStatus.Running;
                }
            }
        }
    }

    private OpenCodeInstance? FindInstanceByDirectory(string? directory)
    {
        if (string.IsNullOrEmpty(directory)) return null;
        var key = NormalizePath(directory);
        return _instances.Values.FirstOrDefault(i =>
            string.Equals(NormalizePath(i.FolderPath), key, StringComparison.OrdinalIgnoreCase));
    }

    private OpenCodeInstance? FindInstanceBySession(string sessionId)
        => _instances.Values.FirstOrDefault(i =>
            string.Equals(i.SessionId, sessionId, StringComparison.Ordinal));

    // --- subprocess ---

    private void StartSubprocess()
    {
        var exe = string.IsNullOrWhiteSpace(_settings.OpenCodeExecutable) ? "opencode" : _settings.OpenCodeExecutable;
        var args = $"serve --hostname \"{_settings.Host}\" --port {_settings.Port}";

        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = exe,
                Arguments = args,
                UseShellExecute = false,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };
            if (!string.IsNullOrWhiteSpace(_settings.AuthToken))
            {
                psi.EnvironmentVariables["OPENCODE_SERVER_PASSWORD"] = _settings.AuthToken;
            }

            _process = new Process { StartInfo = psi, EnableRaisingEvents = true };
            _process.Exited += (_, _) => SetConnected(false);
            _process.Start();
            Log.Logger.Information("OpenCodeServeService: started {Exe} {Args} (pid {Pid})", exe, args, _process.Id);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "OpenCodeServeService: failed to start serve subprocess");
            _process = null;
        }
    }

    // --- state changes / repo mirroring ---

    private void SetConnected(bool value)
    {
        if (IsConnected == value) return;
        IsConnected = value;
        ConnectionChanged?.Invoke(this, value);
    }

    private void OnInstanceStatusChanged(OpenCodeInstance instance, OpenCodeInstanceStatus _)
    {
        ApplyToRepo(instance);
        InstanceChanged?.Invoke(instance);
    }

    private void OnInstanceAutoApproveChanged(OpenCodeInstance instance, bool _)
    {
        ApplyToRepo(instance);
        InstanceChanged?.Invoke(instance);
    }

    /// <summary>Pushes the instance's status + auto-approve onto the matching repo (if loaded).</summary>
    private void ApplyToRepo(OpenCodeInstance instance)
    {
        var repo = _repoService.Repos.FirstOrDefault(r =>
            string.Equals(r.FolderPath, instance.FolderPath, StringComparison.OrdinalIgnoreCase));
        if (repo is null) return;
        repo.OpenCodeStatus = instance.Status;
        repo.OpenCodeAutoApprove = instance.AutoApprove;
    }

    private void ClearRepoStatus(string folderPath)
    {
        var repo = _repoService.Repos.FirstOrDefault(r =>
            string.Equals(r.FolderPath, folderPath, StringComparison.OrdinalIgnoreCase));
        if (repo is null) return;
        repo.OpenCodeStatus = OpenCodeInstanceStatus.Stopped;
        repo.OpenCodeAutoApprove = false;
    }

    // --- helpers ---

    private static string BuildBaseUrl(OpenCodeServeSettings s)
        => $"http://{s.Host}:{s.Port}";

    /// <summary>Normalizes a path for comparison (trim + OS-invariant trailing-separator + casing).</summary>
    private static string NormalizePath(string? path)
    {
        if (string.IsNullOrWhiteSpace(path)) return string.Empty;
        return path.TrimEnd('\\', '/').Trim();
    }
}
