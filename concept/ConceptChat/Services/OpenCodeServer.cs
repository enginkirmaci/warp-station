using System.Diagnostics;
using System.Net.Http;

namespace ConceptChat.Services;

/// <summary>
/// Spawns <c>opencode serve</c> for a working folder, or attaches when the chosen
/// port already answers. Bash/edit/webfetch permissions are forced to "ask" through
/// OPENCODE_CONFIG_CONTENT (verified against opencode 1.18.31; there is no --config
/// flag on serve) so the concept always exercises the permission flow without
/// touching the user's global config or the working folder.
/// </summary>
public sealed class OpenCodeServer
{
    public const string AskConfig =
        """{"$schema":"https://opencode.ai/config.json","permission":{"edit":"ask","bash":"ask","webfetch":"ask"}}""";

    private Process? _process;
    private readonly HttpClient _http = new() { Timeout = TimeSpan.FromSeconds(2) };

    public string BaseUrl { get; private set; } = "";
    public bool Spawned { get; private set; }
    public bool IsRunning { get; private set; }

    public async Task StartAsync(string executable, string folder, int port, Action<string> log, CancellationToken ct)
    {
        BaseUrl = $"http://127.0.0.1:{port}";
        if (await IsHealthyAsync(ct))
        {
            Spawned = false;
            IsRunning = true;
            log($"attaching to a healthy opencode server at {BaseUrl}");
            return;
        }

        var psi = new ProcessStartInfo
        {
            FileName = executable,
            Arguments = $"serve --hostname 127.0.0.1 --port {port}",
            // serve inherits its config and project scope from the working directory.
            WorkingDirectory = Directory.Exists(folder) ? folder : Environment.CurrentDirectory,
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
        };
        psi.Environment["OPENCODE_CONFIG_CONTENT"] = AskConfig;
        psi.Environment.Remove("ELECTRON_RUN_AS_NODE");

        _process = Process.Start(psi) ?? throw new InvalidOperationException($"'{executable}' failed to start.");
        // OnClosing is skipped on SIGTERM; the runtime raises ProcessExit for it.
        AppDomain.CurrentDomain.ProcessExit += (_, _) => Stop();
        _process.OutputDataReceived += (_, e) => { if (e.Data is not null) log(e.Data); };
        _process.ErrorDataReceived += (_, e) => { if (e.Data is not null) log(e.Data); };
        _process.BeginOutputReadLine();
        _process.BeginErrorReadLine();
        Spawned = true;

        var deadline = TimeSpan.FromSeconds(25);
        var start = Stopwatch.StartNew();
        while (start.Elapsed < deadline)
        {
            ct.ThrowIfCancellationRequested();
            if (_process.HasExited)
                throw new InvalidOperationException($"opencode serve exited with code {_process.ExitCode}.");
            if (await IsHealthyAsync(ct))
            {
                IsRunning = true;
                return;
            }
            await Task.Delay(300, ct);
        }

        Stop();
        throw new TimeoutException("opencode serve did not become healthy within 25 s.");
    }

    public async Task<bool> IsHealthyAsync(CancellationToken ct)
    {
        try
        {
            using var resp = await _http.GetAsync($"{BaseUrl}/global/health", ct);
            return resp.IsSuccessStatusCode;
        }
        catch
        {
            return false;
        }
    }

    public void Stop()
    {
        if (Spawned && _process is { HasExited: false } p)
        {
            try
            {
                p.Kill(entireProcessTree: true);
            }
            catch
            {
                // already gone
            }
        }
        _process = null;
        Spawned = false;
        IsRunning = false;
    }
}
