using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services.OpenCode;

/// <summary>
/// Runs wand prompts through an <c>opencode serve</c> process instead of the one-shot
/// CLI: one server per run (spawned, used, killed — nothing idles between runs), one
/// session per prompt, the answer read from the message POST's final assistant
/// message. The reason for the detour is the server started with
/// <see cref="AskPermissionConfig"/>: tool requests surface as permission asks on the
/// SSE stream, which this service routes to the app's popup (allow-all / reject) —
/// the headless CLI could only auto-deny them. Verified live against opencode 1.18.32:
/// without the forced config the server auto-ALLOWS every tool, so the popup feature
/// stands or falls with this env var.
/// </summary>
public class OpenCodeServeRunService : IOpenCodeRunService
{
    /// <summary>Hard cap for one generation: a model turn plus headroom for a few
    /// asks auto-rejected by the popup (15 s each). A hung session must still not pin
    /// the wand button forever.</summary>
    private static readonly TimeSpan RunTimeout = TimeSpan.FromSeconds(150);

    private const string AskPermissionConfig =
        """{"$schema":"https://opencode.ai/config.json","permission":{"edit":"ask","bash":"ask","webfetch":"ask"}}""";

    private static readonly HttpClient Http = new() { Timeout = Timeout.InfiniteTimeSpan };

    private readonly IOpenCodePermissionPrompt _permissionPrompt;

    /// <summary>The in-flight runs' cancellation scopes, for <see cref="Stop"/>.</summary>
    private readonly object _gate = new();
    private readonly List<CancellationTokenSource> _activeRuns = new();

    public OpenCodeServeRunService(IOpenCodePermissionPrompt permissionPrompt)
        => _permissionPrompt = permissionPrompt;

    /// <inheritdoc/>
    public void Stop()
    {
        lock (_gate)
        {
            foreach (var run in _activeRuns)
            {
                try { run.Cancel(); }
                catch (ObjectDisposedException) { /* the run just completed on its own */ }
            }

            _activeRuns.Clear();
        }
    }

    /// <inheritdoc/>
    public async Task<string?> RunAsync(
        string? executable,
        string? model,
        string prompt,
        CancellationToken cancellationToken = default,
        string? workingDirectory = null)
    {
        var (exe, resolved) = ProcessRunner.LocateCli(executable, ReposSettings.DefaultOpenCodeExecutable, nameof(OpenCodeServeRunService), warnWhenMissing: false);
        if (resolved is null)
        {
            return null;
        }

        // Same scope pattern as OpenCodeRunService: Stop() cancels the scopes of runs
        // whose owning ViewModel nobody can reach at shutdown; the kill registration
        // below fires synchronously on the cancelling thread, so the serve process —
        // and the model workers under it — die even if this await never resumes.
        using var runScope = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        lock (_gate) _activeRuns.Add(runScope);

        Process? process = null;
        try
        {
            process = StartServer(resolved, out var baseUrl, workingDirectory);
            using var killOnCancel = runScope.Token.Register(
                static p => { try { ((Process)p!).Kill(entireProcessTree: true); } catch { /* already exited */ } },
                process);
            // Undrained stderr would fill its pipe and block the server mid-write.
            // Drives to completion regardless of teardown so the pipe survives to kill.
            var stderrTask = process.StandardError.ReadToEndAsync(CancellationToken.None);

            if (!await WaitHealthyAsync(baseUrl, process, stderrTask, exe, runScope.Token))
            {
                return null;
            }

            var (providerId, modelId) = await ResolveModelAsync(baseUrl, model, runScope.Token);

            // The SSE stream must be open before the prompt is sent — permission asks
            // are answered on it, and a late subscription could miss the first ask.
            using var pumpScope = CancellationTokenSource.CreateLinkedTokenSource(runScope.Token);
            using var sseResponse = await Http.GetAsync($"{baseUrl}/event", HttpCompletionOption.ResponseHeadersRead, pumpScope.Token);
            using var sseReader = new StreamReader(await sseResponse.Content.ReadAsStreamAsync(pumpScope.Token));

            var run = new RunState();
            var pump = PumpEventsAsync(sseReader, baseUrl, run, pumpScope.Token);

            run.SessionId = await CreateSessionAsync(baseUrl, runScope.Token);
            var messageTask = SendMessageAsync(baseUrl, run.SessionId, prompt, providerId, modelId, runScope.Token);
            // Faults are handled in the completion loop below; this only keeps an
            // abandoned POST (timeout/cancel path) from dying unobserved.
            _ = messageTask.ContinueWith(static t => _ = t.Exception, TaskContinuationOptions.OnlyOnFaulted);

            string? answer;
            try
            {
                answer = await AwaitAnswerAsync(messageTask, run, exe, runScope.Token);
            }
            finally
            {
                // End the pump before the reader is disposed, on every exit path — a
                // pump still reading a disposed stream would only produce noise.
                pumpScope.Cancel();
                try { await pump; } catch { /* the pump swallows its own teardown */ }
            }

            return answer;
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested || runScope.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            Log.Error(ex, "OpenCodeServeRunService: '{Exe} serve' run failed", exe);
            return null;
        }
        finally
        {
            // Every exit path kills the per-run server: Dispose alone would orphan
            // it, and the kill-on-cancel registration only covers cancellation.
            if (process is not null)
            {
                try { process.Kill(entireProcessTree: true); } catch { /* already exited */ }
                process.Dispose();
            }

            lock (_gate) _activeRuns.Remove(runScope);
        }
    }

    /// <summary>Spawns <c>opencode serve</c> on a free localhost port. The process is
    /// returned already started; the caller owns killing it (every exit path kills —
    /// the per-run server must never outlive its run).</summary>
    private Process StartServer(string resolved, out string baseUrl, string? workingDirectory)
    {
        var port = FindFreePort();
        baseUrl = $"http://127.0.0.1:{port}";
        var psi = new ProcessStartInfo
        {
            FileName = resolved,
            UseShellExecute = false,
            CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            // Same stdin guard as OpenCodeRunService: a debugger-hosted live stdin
            // pipe must never be able to block the child.
            RedirectStandardInput = true,
            WorkingDirectory = Directory.Exists(workingDirectory) ? workingDirectory : Environment.CurrentDirectory,
        };
        psi.ArgumentList.Add("serve");
        psi.ArgumentList.Add("--hostname");
        psi.ArgumentList.Add("127.0.0.1");
        psi.ArgumentList.Add("--port");
        psi.ArgumentList.Add(port.ToString(CultureInfo.InvariantCulture));
        // OPENCODE_CONFIG_CONTENT rides IN ADDITION to the user's config; without the
        // forced "ask" the server silently allows every tool and no ask ever fires.
        psi.Environment["OPENCODE_CONFIG_CONTENT"] = AskPermissionConfig;
        psi.EnvironmentVariables.Remove(ProcessRunner.ElectronRunAsNodeVariable);

        var process = new Process { StartInfo = psi };
        process.Start();
        process.StandardInput.Close();
        return process;
    }

    private static int FindFreePort()
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        try { return ((IPEndPoint)listener.LocalEndpoint).Port; }
        finally { listener.Stop(); }
    }

    /// <summary>Polls <c>/global/health</c> until the server answers (300 ms steps,
    /// 25 s deadline) or the process dies early (its stderr tail then names the cause).</summary>
    private static async Task<bool> WaitHealthyAsync(
        string baseUrl, Process process, Task<string> stderrTask, string exe, CancellationToken cancellationToken)
    {
        var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(25);
        while (DateTime.UtcNow < deadline)
        {
            if (process.HasExited)
            {
                var tail = (await stderrTask).Trim();
                Log.Error("OpenCodeServeRunService: '{Exe} serve' exited early ({ExitCode}): {Stderr}",
                    exe, process.ExitCode, tail.Length > 500 ? tail[..500] : tail);
                return false;
            }

            try
            {
                using var healthCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
                healthCts.CancelAfter(TimeSpan.FromSeconds(2));
                using var response = await Http.GetAsync($"{baseUrl}/global/health", healthCts.Token);
                if (response.IsSuccessStatusCode)
                {
                    return true;
                }
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                throw;
            }
            catch (Exception)
            {
                // Not healthy yet — poll again until the deadline.
            }

            await Task.Delay(300, CancellationToken.None);
        }

        Log.Error("OpenCodeServeRunService: '{Exe} serve' did not become healthy within 25s", exe);
        return false;
    }

    /// <summary>Translates the configured catalog id ("opencode-go/glm-5.3-flash" —
    /// the CLI's alias form) into the server's real {providerID, modelID} pair. The
    /// server rejects alias providers ("Model not found: opencode-go/…"), so exact
    /// provider match first, then a model-id match across providers (prefix heuristic
    /// when several providers offer it); no match falls back to the server default.</summary>
    private static async Task<(string? ProviderId, string? ModelId)> ResolveModelAsync(
        string baseUrl, string? model, CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(model))
        {
            return (null, null);
        }

        try
        {
            using var cts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            cts.CancelAfter(TimeSpan.FromSeconds(10));
            using var response = await Http.GetAsync($"{baseUrl}/config/providers", cts.Token);
            response.EnsureSuccessStatusCode();
            using var doc = JsonDocument.Parse(await response.Content.ReadAsStringAsync(cts.Token));

            var providers = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            var models = new List<(string Provider, string Id)>();
            if (doc.RootElement.TryGetProperty("providers", out var list) && list.ValueKind == JsonValueKind.Array)
            {
                foreach (var provider in list.EnumerateArray())
                {
                    var id = GetString(provider, "id");
                    if (id is null) continue;
                    providers.Add(id);
                    if (provider.TryGetProperty("models", out var entryModels))
                    {
                        if (entryModels.ValueKind == JsonValueKind.Object)
                        {
                            foreach (var entry in entryModels.EnumerateObject())
                            {
                                models.Add((id, entry.Name));
                            }
                        }
                        else if (entryModels.ValueKind == JsonValueKind.Array)
                        {
                            foreach (var entry in entryModels.EnumerateArray())
                            {
                                if (GetString(entry, "id") is { } modelId) models.Add((id, modelId));
                            }
                        }
                    }
                }
            }

            var separator = model.IndexOf('/');
            var wantedProvider = separator > 0 ? model[..separator] : null;
            var wantedModel = separator > 0 ? model[(separator + 1)..] : model;

            if (wantedProvider is not null && providers.Contains(wantedProvider) &&
                models.Any(m => string.Equals(m.Provider, wantedProvider, StringComparison.OrdinalIgnoreCase) && m.Id == wantedModel))
            {
                return (wantedProvider, wantedModel);
            }

            var matches = models.Where(m => m.Id == wantedModel).ToList();
            if (matches.Count == 1)
            {
                return (matches[0].Provider, matches[0].Id);
            }

            if (matches.Count > 1)
            {
                // Stale catalog ids can name a provider the server no longer knows
                // ("opencode-go/…"); among the providers offering the model, the one
                // sharing the longest prefix with the wanted provider id is the best
                // guess — the choice is logged since it is a heuristic.
                var chosen = matches
                    .OrderByDescending(m => CommonPrefixLength(m.Provider, wantedProvider ?? string.Empty))
                    .First();
                Log.Information("OpenCodeServeRunService: model '{Model}' resolved to {Provider}/{ModelId}", model, chosen.Provider, chosen.Id);
                return (chosen.Provider, chosen.Id);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            Log.Warning(ex, "OpenCodeServeRunService: could not resolve model '{Model}' against the server catalog", model);
        }

        Log.Warning("OpenCodeServeRunService: model '{Model}' not found on the server — using the server default", model);
        return (null, null);
    }

    private static async Task<string> CreateSessionAsync(string baseUrl, CancellationToken cancellationToken)
    {
        using var cts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        cts.CancelAfter(TimeSpan.FromSeconds(30));
        using var response = await Http.PostAsync($"{baseUrl}/session", JsonContent("""{"title":"DevTools generation"}"""), cts.Token);
        response.EnsureSuccessStatusCode();
        using var doc = JsonDocument.Parse(await response.Content.ReadAsStringAsync(cts.Token));
        var id = GetString(doc.RootElement, "id")
            ?? (doc.RootElement.TryGetProperty("info", out var info) ? GetString(info, "id") : null)
            ?? throw new InvalidOperationException("session response carried no id");
        return id;
    }

    /// <summary>Sends the prompt. The POST resolves only when the turn is finished and
    /// carries the FINAL assistant message (live-verified: intermediate tool-call
    /// messages are not returned), so the answer comes straight from its text parts.</summary>
    private static async Task<JsonDocument> SendMessageAsync(
        string baseUrl, string sessionId, string prompt, string? providerId, string? modelId, CancellationToken cancellationToken)
    {
        var body = new Dictionary<string, object>
        {
            ["parts"] = new List<object> { new { type = "text", text = prompt } },
        };
        if (providerId is not null && modelId is not null)
        {
            body["model"] = new { providerID = providerId, modelID = modelId };
        }

        using var request = new HttpRequestMessage(HttpMethod.Post, $"{baseUrl}/session/{sessionId}/message")
        {
            Content = JsonContent(JsonSerializer.Serialize(body, JsonSerializerOptions.Web)),
        };
        using var response = await Http.SendAsync(request, cancellationToken);
        var bodyText = await response.Content.ReadAsStringAsync(cancellationToken);
        if (!response.IsSuccessStatusCode)
        {
            throw new HttpRequestException($"message POST returned {(int)response.StatusCode}: {(bodyText.Length > 300 ? bodyText[..300] : bodyText)}");
        }

        return JsonDocument.Parse(bodyText);
    }

    /// <summary>Waits for the run to settle: the message POST (success or failure), a
    /// session error on the stream, or the run timeout. Returns the answer text, or
    /// null after logging the reason.</summary>
    private static async Task<string?> AwaitAnswerAsync(
        Task<JsonDocument> messageTask, RunState run, string exe, CancellationToken cancellationToken)
    {
        var timeoutTask = Task.Delay(RunTimeout, cancellationToken);
        var completed = await Task.WhenAny(messageTask, run.Error.Task, timeoutTask);
        await completed; // rethrows: a cancelled scope surfaces as cancellation, a faulted POST as its exception

        if (completed == timeoutTask)
        {
            Log.Error("OpenCodeServeRunService: '{Exe} serve' run timed out after {Seconds}s", exe, RunTimeout.TotalSeconds);
            return null;
        }

        if (completed == run.Error.Task)
        {
            Log.Error("OpenCodeServeRunService: session error: {Error}", run.Error.Task.Result);
            return null;
        }

        using var response = await messageTask;
        var answer = ExtractAnswer(response.RootElement) ?? run.ExtractLastAssistantText();
        if (answer is null)
        {
            Log.Error("OpenCodeServeRunService: run completed without a text answer");
            return null;
        }

        return answer.Trim();
    }

    private static string? ExtractAnswer(JsonElement root)
    {
        if (!root.TryGetProperty("parts", out var parts) || parts.ValueKind != JsonValueKind.Array)
        {
            return null;
        }

        var texts = parts.EnumerateArray()
            .Where(p => GetString(p, "type") == "text")
            .Select(p => GetString(p, "text"))
            .Where(t => !string.IsNullOrWhiteSpace(t))
            .ToList();
        return texts.Count > 0 ? string.Join('\n', texts) : null;
    }

    /// <summary>Reads the SSE stream until the server dies or the run ends: routes
    /// permission asks to the popup (their replies unblock the turn), session errors
    /// to the completion loop, and accumulates assistant text as the answer fallback
    /// should a future opencode stop returning the final message from the POST.</summary>
    private async Task PumpEventsAsync(StreamReader reader, string baseUrl, RunState run, CancellationToken cancellationToken)
    {
        try
        {
            while (await reader.ReadLineAsync(cancellationToken) is { } line)
            {
                if (line.Length <= 5 || !line.StartsWith("data:", StringComparison.Ordinal)) continue;
                var payload = line[5..].StartsWith(' ') ? line[6..] : line[5..];

                JsonDocument doc;
                try { doc = JsonDocument.Parse(payload); }
                catch { continue; }

                using (doc)
                {
                    var type = GetString(doc.RootElement, "type");
                    var properties = doc.RootElement.TryGetProperty("properties", out var p) ? p : doc.RootElement;
                    switch (type)
                    {
                        case "permission.asked" or "permission.v2.asked":
                            await HandleAskAsync(properties, type == "permission.v2.asked", baseUrl, run, cancellationToken);
                            break;

                        case "session.error" when run.MatchesSession(properties):
                            var error = properties.TryGetProperty("error", out var e) ? e : (JsonElement?)null;
                            var message = error is null ? null
                                : GetString(error.Value, "message")
                                    ?? (error.Value.TryGetProperty("data", out var d) ? GetString(d, "message") : null)
                                    ?? GetString(error.Value, "name");
                            run.Error.TrySetResult(message ?? "unknown session error");
                            break;

                        case "message.updated" when run.MatchesSession(properties)
                            && properties.TryGetProperty("info", out var info):
                            run.TrackMessage(info);
                            break;

                        case "message.part.updated" when run.MatchesSession(properties)
                            && properties.TryGetProperty("part", out var part):
                            run.TrackPart(part);
                            break;
                    }
                }
            }
        }
        catch (OperationCanceledException) { /* the run ended — the stream dies with the server */ }
        catch (Exception ex)
        {
            // The stream ending early is survivable (the POST still completes); the
            // file sink keeps Error only, so log at Error to keep a diagnosis trail.
            Log.Error(ex, "OpenCodeServeRunService: event stream failed");
        }
    }

    private async Task HandleAskAsync(JsonElement properties, bool isV2, string baseUrl, RunState run, CancellationToken cancellationToken)
    {
        var id = GetString(properties, "id");
        if (id is null || !run.MatchesSession(properties))
        {
            return;
        }

        var kind = GetString(properties, "permission") ?? GetString(properties, "action") ?? "tool";
        var allow = await _permissionPrompt.AskAsync(
            new OpenCodePermissionRequest(run.SessionId ?? string.Empty, id, kind, DescribeAskDetail(properties)),
            cancellationToken);

        // Reply shapes are versioned: v1 (the only shape 1.18.x emits for these runs)
        // posts {response} under the session, v2 posts {reply} to the global route.
        var payload = isV2
            ? $"{{\"reply\":\"{(allow ? "always" : "reject")}\"}}"
            : $"{{\"response\":\"{(allow ? "always" : "reject")}\"}}";
        var route = isV2 ? $"{baseUrl}/permission/{id}/reply" : $"{baseUrl}/session/{run.SessionId}/permissions/{id}";
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Post, route) { Content = JsonContent(payload) };
            using var response = await Http.SendAsync(request, cancellationToken);
            if (!response.IsSuccessStatusCode)
            {
                Log.Error("OpenCodeServeRunService: permission reply {Id} returned {Status}", id, (int)response.StatusCode);
            }
        }
        catch (Exception ex)
        {
            // The run is being torn down or the server died — the ask no longer matters.
            Log.Debug(ex, "OpenCodeServeRunService: permission reply {Id} failed", id);
        }
    }

    /// <summary>Pulls a one-line human detail out of the ask's versioned shape (v1
    /// metadata.command/filepath/url, v2 resources[], patterns as the last resort).</summary>
    private static string? DescribeAskDetail(JsonElement properties)
    {
        if (properties.TryGetProperty("metadata", out var metadata) && metadata.ValueKind == JsonValueKind.Object)
        {
            foreach (var key in new[] { "command", "filepath", "path", "url" })
            {
                if (GetString(metadata, key) is { } value && value.Length > 0)
                {
                    return Cap(value);
                }
            }
        }

        if (properties.TryGetProperty("resources", out var resources) && resources.ValueKind == JsonValueKind.Array)
        {
            var items = resources.EnumerateArray()
                .Select(r => GetString(r, "path") ?? GetString(r, "url") ?? GetString(r, "id"))
                .Where(v => !string.IsNullOrEmpty(v))
                .Take(5)
                .ToList();
            if (items.Count > 0) return Cap(string.Join(", ", items!));
        }

        if (properties.TryGetProperty("patterns", out var patterns) && patterns.ValueKind == JsonValueKind.Array)
        {
            var items = patterns.EnumerateArray()
                .Where(p => p.ValueKind == JsonValueKind.String)
                .Select(p => p.GetString())
                .Where(v => !string.IsNullOrEmpty(v))
                .Take(5)
                .ToList();
            if (items.Count > 0) return Cap(string.Join(", ", items!));
        }

        return null;
    }

    private static string Cap(string value) => value.Length <= 300 ? value : value[..300] + "…";

    private static int CommonPrefixLength(string left, string right)
    {
        var length = Math.Min(left.Length, right.Length);
        var count = 0;
        while (count < length && char.ToLowerInvariant(left[count]) == char.ToLowerInvariant(right[count]))
        {
            count++;
        }

        return count;
    }

    private static StringContent JsonContent(string json) => new(json, Encoding.UTF8, "application/json");

    private static string? GetString(JsonElement element, string name)
        => element.TryGetProperty(name, out var value) && value.ValueKind == JsonValueKind.String ? value.GetString() : null;

    /// <summary>Per-run accumulation of the SSE stream: message roles and part texts,
    /// keyed by opencode's ids. All access is from the pump thread only.</summary>
    private sealed class RunState
    {
        public string? SessionId;

        /// <summary>Completed with a human-readable description when the session errors.</summary>
        public readonly TaskCompletionSource<string> Error = new(TaskCreationOptions.RunContinuationsAsynchronously);

        private readonly List<string> _messageOrder = new();
        private readonly Dictionary<string, string> _roles = new();
        private readonly List<string> _partOrder = new();
        private readonly Dictionary<string, (string MessageId, string Type, string Text)> _parts = new();

        public bool MatchesSession(JsonElement properties)
        {
            // Unattributed events can only belong to this server's single session.
            var sessionId = SessionId is null ? null : GetString(properties, "sessionID");
            return sessionId is null || sessionId == SessionId;
        }

        public void TrackMessage(JsonElement info)
        {
            var id = GetString(info, "id");
            if (id is null) return;
            if (!_roles.ContainsKey(id)) _messageOrder.Add(id);
            _roles[id] = GetString(info, "role") ?? string.Empty;
        }

        public void TrackPart(JsonElement part)
        {
            var id = GetString(part, "id");
            if (id is null) return;
            if (!_parts.ContainsKey(id)) _partOrder.Add(id);
            _parts[id] = (
                GetString(part, "messageID") ?? string.Empty,
                GetString(part, "type") ?? string.Empty,
                GetString(part, "text") ?? string.Empty);
        }

        /// <summary>The last assistant message that carries text, its text parts in
        /// stream order — the streaming equivalent of the POST's final message.</summary>
        public string? ExtractLastAssistantText()
        {
            for (var i = _messageOrder.Count - 1; i >= 0; i--)
            {
                if (_roles.GetValueOrDefault(_messageOrder[i]) != "assistant") continue;
                var texts = _partOrder
                    .Select(partId => _parts[partId])
                    .Where(p => p.MessageId == _messageOrder[i] && p.Type == "text" && p.Text.Length > 0)
                    .Select(p => p.Text)
                    .ToList();
                if (texts.Count > 0)
                {
                    return string.Join('\n', texts);
                }
            }

            return null;
        }
    }
}
