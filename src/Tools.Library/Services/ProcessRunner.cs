using System.Diagnostics;
using Serilog;

namespace Tools.Library.Services;

/// <summary>
/// Single runner for the app's one-shot CLI invocations (git, gh, dotnet nuget, opencode):
/// spawn with redirected pipes, drain both pipes concurrently (an undrained stderr pipe
/// fills, the child blocks writing its progress, and the run "times out"), bound the run
/// with a timeout, and kill the child's entire process tree when the bound expires. It
/// replaces the spawn/timeout/kill boilerplate that used to be copy-pasted across
/// <see cref="GitStatusService"/>, <see cref="GitHubService"/>, <see cref="NugetLocalService"/>
/// and the opencode services.
/// <para>
/// Failure reporting is deliberately non-throwing: spawn failures surface as a result with
/// a <see langword="null"/> <see cref="ProcessRunResult.ExitCode"/>, while a
/// <c>Win32Exception</c> from the spawn (executable missing) propagates so callers
/// can keep their per-service "CLI unavailable" handling. External cancellation propagates
/// as <see cref="OperationCanceledException"/> — like the timeouts, it is the caller's job
/// to decide what a cancelled run means.
/// </para>
/// </summary>
public static class ProcessRunner
{
    /// <summary>
    /// Set by Electron hosts (VS Code & forks, some IDE terminals) that Tools may have
    /// been launched from; it leaks into every child and turns Electron-packaged CLIs
    /// (e.g. an AppImage of zcode) into bare Node processes. Hoisted here from
    /// <see cref="ProcessLauncher"/> so every spawn path strips via the same constant.
    /// </summary>
    public const string ElectronRunAsNodeVariable = "ELECTRON_RUN_AS_NODE";

    /// <summary>
    /// Resolves a CLI executable for spawning. Empty/whitespace names fall back to
    /// <paramref name="fallback"/>, then <see cref="ExecutableDefaults.Locate"/> resolves
    /// the name: the GUI process runs with a minimal PATH (no shell rc files), so a bare
    /// name must also be looked up in the user-level install dirs, and spawning needs the
    /// resolved path either way. When nothing resolvable is found, the lookup is logged
    /// (warning when <paramref name="warnWhenMissing"/>, debug otherwise) and the returned
    /// path is <see langword="null"/>.
    /// </summary>
    /// <param name="configured">The configured executable name or path, if any.</param>
    /// <param name="fallback">The bare CLI name to use when nothing is configured.</param>
    /// <param name="logName">Service name prefixing the "not found" log message.</param>
    /// <param name="warnWhenMissing">Warning instead of debug for the "not found" log —
    /// set false when the caller treats a missing CLI as routine.</param>
    /// <returns>The resolved executable to spawn, or null when it could not be located.</returns>
    public static (string Exe, string? ResolvedPath) LocateCli(
        string? configured,
        string fallback,
        string logName,
        bool warnWhenMissing = true)
    {
        var exe = string.IsNullOrWhiteSpace(configured) ? fallback : configured;

        var resolved = ExecutableDefaults.Locate(exe);
        if (resolved is null)
        {
            if (warnWhenMissing)
            {
                Log.Logger.Warning(
                    "{LogName}: '{Exe}' was not found on PATH or in the common user install folders (~/.local/bin, ~/.opencode/bin, …)",
                    logName, exe);
            }
            else
            {
                Log.Logger.Debug("{LogName}: '{Exe}' not found on PATH or in the user install folders", logName, exe);
            }
        }

        return (exe, resolved);
    }

    /// <summary>Runs one child process to completion and captures its output.</summary>
    /// <param name="options">The spawn description (executable, arguments, bounds, env).</param>
    /// <param name="cancellationToken">External cancellation; cancels the wait and drains,
    /// propagating <see cref="OperationCanceledException"/> (the child itself is not killed
    /// on external cancel — pass <see cref="ProcessRunOptions.KillOnCancel"/> for that).</param>
    /// <returns>
    /// The child's exit code and both pipe contents. On timeout the child's tree is killed
    /// and the result carries <see cref="ProcessRunResult.TimedOut"/> with a
    /// <see langword="null"/> exit code and empty output. Spawn failures return a
    /// <see langword="null"/> exit code without timing out.
    /// </returns>
    public static async Task<ProcessRunResult> RunAsync(ProcessRunOptions options, CancellationToken cancellationToken = default)
    {
        var startInfo = new ProcessStartInfo
        {
            FileName = options.FileName,
            Arguments = options.Arguments,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            // Only requested when the caller closes the child's stdin: a closed pipe is an
            // instant EOF, same as a terminal run — CLIs that read piped stdin would
            // otherwise hang on a live pipe (e.g. one kept open under a debugger).
            RedirectStandardInput = options.CloseStandardInput,
            UseShellExecute = false,
            CreateNoWindow = true,
        };

        if (options.WorkingDirectory is not null)
        {
            startInfo.WorkingDirectory = options.WorkingDirectory;
        }

        if (options.EnvironmentVariables is not null)
        {
            foreach (var (name, value) in options.EnvironmentVariables)
            {
                startInfo.Environment[name] = value;
            }
        }

        if (options.StripElectronVariable)
        {
            startInfo.EnvironmentVariables.Remove(ElectronRunAsNodeVariable);
        }

        var process = Process.Start(startInfo);
        if (process is null)
        {
            return new ProcessRunResult(ExitCode: null, string.Empty, string.Empty, TimedOut: false);
        }

        using (process)
        {
            if (options.CloseStandardInput)
            {
                process.StandardInput.Close();
            }

            // Kill-on-cancel must not depend on an async continuation noticing the token:
            // the Register callback fires synchronously on whichever thread cancels (app
            // shutdown, repo switch), so the child — and the workers it spawns — dies even
            // if the awaiting call never resumes again.
            using var killOnCancel = options.KillOnCancel
                ? cancellationToken.Register(
                    static p => { try { ((Process)p!).Kill(entireProcessTree: true); } catch { /* already exited */ } },
                    process)
                : default(CancellationTokenRegistration);

            using var timeoutCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            if (options.Timeout is { } timeout)
            {
                timeoutCts.CancelAfter(timeout);
            }

            try
            {
                var maxChars = options.MaxOutputChars;
                var stdoutTask = maxChars is null
                    ? process.StandardOutput.ReadToEndAsync(timeoutCts.Token)
                    : ReadCappedAsync(process.StandardOutput, maxChars.Value, process);
                var stderrTask = process.StandardError.ReadToEndAsync(timeoutCts.Token);
                await process.WaitForExitAsync(timeoutCts.Token);
                await Task.WhenAll(stdoutTask, stderrTask);
                return new ProcessRunResult(
                    process.ExitCode,
                    stdoutTask.Result,
                    stderrTask.Result,
                    TimedOut: false,
                    Truncated: maxChars is { } cap && stdoutTask.Result.Length >= cap);
            }
            catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
            {
                // Timeout, not an external cancel: kill the stray child and report.
                try { process.Kill(entireProcessTree: true); } catch { /* already exited */ }
                return new ProcessRunResult(ExitCode: null, string.Empty, string.Empty, TimedOut: true);
            }
        }
    }

    /// <summary>
    /// Reads stdout until <paramref name="maxChars"/> characters are captured, then
    /// kills the child's whole tree: the pipes EOF, so the stderr drain and the exit
    /// wait complete and the run finishes normally with the truncated head. For
    /// consumers that only need output's head (the commit-message prompt truncates
    /// the staged diff anyway), this avoids materializing a multi-megabyte string.
    /// The killed run's exit code is non-zero — callers check
    /// <see cref="ProcessRunResult.Truncated"/>, not just the exit code.
    /// </summary>
    private static async Task<string> ReadCappedAsync(StreamReader reader, int maxChars, Process process)
    {
        var builder = new StringBuilder(maxChars + 1024);
        var buffer = new char[4096];
        while (builder.Length < maxChars)
        {
            var read = await reader.ReadAsync(buffer);
            if (read <= 0) break; // child closed its stdout before the cap
            builder.Append(buffer, 0, Math.Min(read, maxChars - builder.Length));
        }

        if (builder.Length >= maxChars)
        {
            try { process.Kill(entireProcessTree: true); } catch { /* already exited */ }
        }
        return builder.ToString();
    }
}

/// <summary>
/// Spawn description for one <see cref="ProcessRunner.RunAsync"/> invocation.
/// </summary>
public sealed record ProcessRunOptions
{
    /// <summary>The executable to spawn (a bare PATH name or an absolute path).</summary>
    public required string FileName { get; init; }

    /// <summary>The command line passed to the child (shell-style string, as before).</summary>
    public string Arguments { get; init; } = string.Empty;

    /// <summary>The child's working directory; <see langword="null"/> inherits the app's.</summary>
    public string? WorkingDirectory { get; init; }

    /// <summary>
    /// Upper bound for the run; on expiry the child's ENTIRE process tree is killed and the
    /// result reports <see cref="ProcessRunResult.TimedOut"/>. <see langword="null"/> runs
    /// unbounded (bounded only by external cancellation) — avoid for CLIs that can hang.
    /// </summary>
    public TimeSpan? Timeout { get; init; }

    /// <summary>Environment additions/overrides for the child (e.g. GIT_TERMINAL_PROMPT=0).</summary>
    public IReadOnlyDictionary<string, string>? EnvironmentVariables { get; init; }

    /// <summary>
    /// Removes <see cref="ProcessRunner.ElectronRunAsNodeVariable"/> from the child's
    /// environment (the Electron-host leak guard). Only opt in for Electron-packaged CLIs —
    /// git/gh/dotnet neither need nor notice it.
    /// </summary>
    public bool StripElectronVariable { get; init; }

    /// <summary>
    /// Redirects the child's stdin and closes it immediately, giving CLIs that read piped
    /// stdin an instant EOF instead of a hang (the <c>opencode run</c> shape).
    /// </summary>
    public bool CloseStandardInput { get; init; }

    /// <summary>
    /// Kills the child's entire process tree synchronously — on whichever thread cancels —
    /// when <paramref name="cancellationToken"/> fires, instead of only aborting the wait.
    /// </summary>
    public bool KillOnCancel { get; init; }

    /// <summary>
    /// Optional read cap for stdout: once captured output reaches the cap the child's
    /// tree is killed and the run completes with the head (<see cref="ProcessRunResult.Truncated"/>
    /// flags it — the killed exit code is non-zero). <see langword="null"/> captures everything.
    /// </summary>
    public int? MaxOutputChars { get; init; }
}

/// <summary>Outcome of one <see cref="ProcessRunner.RunAsync"/> invocation.</summary>
/// <param name="ExitCode">
/// The child's exit code, or <see langword="null"/> when it never ran to completion
/// (spawn failure or timeout).
/// </param>
/// <param name="StandardOutput">The child's stdout — everything it wrote, or the first
/// <see cref="ProcessRunOptions.MaxOutputChars"/> characters when the read cap engaged.</param>
/// <param name="StandardError">Everything the child wrote to stderr.</param>
/// <param name="TimedOut">True when the run hit <see cref="ProcessRunOptions.Timeout"/>.</param>
/// <param name="Truncated">True when <see cref="ProcessRunOptions.MaxOutputChars"/> engaged:
/// stdout holds only the head and the child was killed, so <paramref name="ExitCode"/> is
/// the kill's non-zero code.</param>
public sealed record ProcessRunResult(int? ExitCode, string StandardOutput, string StandardError, bool TimedOut, bool Truncated = false)
{
    /// <summary>True only when the child ran and exited 0 — false for non-zero exits,
    /// timeouts and spawn failures alike.</summary>
    public bool Succeeded => ExitCode == 0;
}
