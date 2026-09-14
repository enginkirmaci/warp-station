using System.ComponentModel;
using Serilog;

namespace Tools.Library.Services;

/// <summary>
/// The git process mechanics behind <see cref="GitStatusService"/>: spawning
/// <c>git</c> with the prompt-proof environment, the per-call timeouts, stderr
/// plumbing and the argument quoting. A missing git binary latches the runner
/// unavailable for the rest of the session instead of failing every repo on every
/// refresh. Pure process work — no parsing, no entity mutation.
/// <para>
/// In-process reads (status, diffs, history) bypass this runner entirely (see
/// <see cref="GitReadService"/>); only mutations, syncs and clones shell out, so the
/// user's hooks, gpg signing and credential helpers keep applying. Every spawn kills
/// its whole process tree when its bound expires OR the wait is cancelled, and
/// <see cref="Stop"/> cancels everything at app shutdown — so no git process outlives
/// the app.
/// </para>
/// </summary>
internal sealed class GitCommandRunner
{
    /// <summary>Upper bound for a single git invocation; a hung repo must not stall the rest.</summary>
    public static readonly TimeSpan ProcessTimeout = TimeSpan.FromSeconds(10);

    /// <summary>
    /// Upper bound for fetch/pull/push: network transfers grow with the payload, unlike
    /// the local probes the 10 s bound is tuned for.
    /// </summary>
    public static readonly TimeSpan SyncTimeout = TimeSpan.FromSeconds(60);

    /// <summary>
    /// Upper bound for clone: a full history download can legitimately run for
    /// minutes, and the runner reports completion-only (no progress streaming), so the
    /// bound must cover the slow realistic case rather than a nice UX latency.
    /// </summary>
    public static readonly TimeSpan CloneTimeout = TimeSpan.FromMinutes(10);

    /// <summary>
    /// Environment for every git child: never block on credential/passphrase prompts —
    /// fail fast instead.
    /// </summary>
    private static readonly IReadOnlyDictionary<string, string> GitEnvironment = new Dictionary<string, string>
    {
        ["GIT_TERMINAL_PROMPT"] = "0",
    };

    /// <summary>Set once <c>git</c> is missing on PATH; subsequent runs become no-ops.</summary>
    private volatile bool _unavailable;

    public bool IsUnavailable => _unavailable;

    /// <summary>
    /// One-way shutdown switch: once cancelled, no new git process is spawned and every
    /// in-flight one is killed (the spawns' kill-on-cancel rides the linked token).
    /// Wired to app shutdown via <see cref="Abstractions.IGitStatusService.Stop"/> so no
    /// git process outlives the app.
    /// </summary>
    private readonly CancellationTokenSource _shutdownCts = new();

    public void Stop() => _shutdownCts.Cancel();

    /// <summary>
    /// Runs <c>git</c> with the given arguments in <paramref name="workingDir"/> and
    /// returns stdout, or <see langword="null"/> on any failure (non-zero exit, timeout,
    /// missing binary, cancellation). Prompts are disabled (<c>GIT_TERMINAL_PROMPT=0</c>)
    /// and locks are not taken (<c>--no-optional-locks</c> is the caller's argument
    /// prefix) so commands never interfere with the user's own git operations.
    /// </summary>
    public async Task<string?> RunAsync(
        string workingDir,
        string arguments,
        CancellationToken cancellationToken,
        TimeSpan? timeout = null,
        ICollection<string>? stderrSink = null,
        int? maxOutputChars = null,
        string? truncationSuffix = null)
    {
        if (_shutdownCts.IsCancellationRequested) return null;

        ProcessRunResult result;
        try
        {
            using var linkedCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, _shutdownCts.Token);
            result = await ProcessRunner.RunAsync(new ProcessRunOptions
            {
                FileName = "git",
                Arguments = arguments,
                WorkingDirectory = workingDir,
                Timeout = timeout ?? ProcessTimeout,
                EnvironmentVariables = GitEnvironment,
                MaxOutputChars = maxOutputChars,
                KillOnCancel = true,
            }, linkedCts.Token);
        }
        catch (Win32Exception ex)
        {
            // git is not installed / not on PATH: disable the service until the next
            // app run instead of failing every repo on every refresh.
            _unavailable = true;
            Log.Logger.Debug(ex, "git executable not found; git status checks disabled");
            return null;
        }
        catch (OperationCanceledException)
        {
            // External cancel (caller token or app shutdown): the kill-on-cancel
            // registration already took the process tree down; degrade like any other
            // failed run instead of throwing past the fire-and-forget call sites.
            return null;
        }

        // Sync failures surface their stderr to the user — same line-splitting as before,
        // so the notification still carries one actionable git line.
        if (result.ExitCode != 0 && !result.Truncated && stderrSink is not null)
        {
            foreach (var line in result.StandardError.Split('\n', StringSplitOptions.RemoveEmptyEntries))
            {
                stderrSink.Add(line);
            }
        }

        // A truncated read is a success by contract: the cap kill makes the exit code
        // non-zero, but the caller asked for (and got) exactly the head it wanted.
        var output = result.ExitCode == 0 || result.Truncated ? result.StandardOutput : null;
        if (output is not null && result.Truncated && truncationSuffix is not null)
        {
            output += truncationSuffix;
        }
        return output;
    }

    /// <summary>
    /// Quotes a git argument when it contains spaces (branch names rarely do, but a
    /// ref with one must not split into two arguments).
    /// </summary>
    public static string Quote(string value)
        => value.Contains(' ') ? $"\"{value}\"" : value;
}
