using System.ComponentModel;
using System.Diagnostics;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Default <see cref="IGitStatusService"/>. Runs a single
/// <c>git status --porcelain=v2 --branch --untracked-files=all</c> per repo (bounded
/// parallelism, per-process timeout) and parses the machine-readable output: header
/// lines carry the branch name and ahead/behind counts, every non-header line is one
/// working-tree change. <c>--untracked-files=all</c> counts every untracked file
/// individually — by default git collapses an untracked directory into a single entry.
/// Results are pushed onto the <see cref="Repo"/> entities from background threads, the
/// same way <c>OpenCodeServeService</c> pushes instance status — CommunityToolkit raises
/// <c>PropertyChanged</c> and the bound cards update without the page VM being involved.
/// <para>
/// A refresh is kicked automatically when <see cref="IRepoService"/> raises
/// <c>Changed</c> outside of a scan, so statuses re-check after every rescan without the
/// page having to coordinate anything.
/// </para>
/// </summary>
public sealed class GitStatusService : IGitStatusService
{
    /// <summary>Upper bound for a single git invocation; a hung repo must not stall the rest.</summary>
    private static readonly TimeSpan ProcessTimeout = TimeSpan.FromSeconds(10);

    /// <summary>How many repos are probed concurrently; keeps process storms off the UI machine.</summary>
    private const int MaxParallelism = 4;

    private readonly IRepoService _repoService;

    /// <summary>Guards <see cref="_isRefreshing"/>/<see cref="_refreshPending"/>.</summary>
    private readonly object _sync = new();

    /// <summary>True while a refresh pass loop is running.</summary>
    private bool _isRefreshing;

    /// <summary>Set when a refresh is requested while one is running; runs another pass after.</summary>
    private bool _refreshPending;

    /// <summary>Set once <c>git</c> is missing on PATH; subsequent refreshes become no-ops.</summary>
    private volatile bool _gitUnavailable;

    public GitStatusService(IRepoService repoService)
    {
        _repoService = repoService;
        _repoService.Changed += OnRepoServiceChanged;
    }

    /// <summary>
    /// Re-checks statuses when fresh repo data arrives (cache load, completed rescan).
    /// Skipped while a scan is in flight — the completion notification follows right after.
    /// </summary>
    private void OnRepoServiceChanged(object? sender, EventArgs e)
    {
        if (_repoService.IsBusy || _repoService.Repos.Count == 0) return;
        _ = RefreshAllAsync();
    }

    /// <inheritdoc/>
    public async Task RefreshAllAsync(CancellationToken cancellationToken = default)
    {
        if (_gitUnavailable) return;

        // Coalesce concurrent triggers: while a pass runs, callers just flag a follow-up
        // pass. The pending check and the runner hand-off are atomic under _sync, so no
        // trigger is ever lost between the last pass and the loop exiting.
        lock (_sync)
        {
            if (_isRefreshing)
            {
                _refreshPending = true;
                return;
            }
            _isRefreshing = true;
        }

        while (true)
        {
            lock (_sync)
            {
                _refreshPending = false;
            }

            await RefreshCoreAsync(cancellationToken);

            lock (_sync)
            {
                if (!_refreshPending || cancellationToken.IsCancellationRequested)
                {
                    _isRefreshing = false;
                    return;
                }
            }
        }
    }

    private async Task RefreshCoreAsync(CancellationToken cancellationToken)
    {
        // Snapshot the list: a rescan may swap RepoService.Repos mid-refresh. Probing a
        // repo that has since been removed is harmless — its entity is simply orphaned.
        var repos = _repoService.Repos
            .Where(r => !string.IsNullOrWhiteSpace(r.FolderPath))
            .ToList();
        if (repos.Count == 0) return;

        try
        {
            using var throttle = new SemaphoreSlim(MaxParallelism);
            var tasks = repos.Select(async repo =>
            {
                await throttle.WaitAsync(cancellationToken);
                try
                {
                    await RefreshRepoAsync(repo, cancellationToken);
                }
                finally
                {
                    throttle.Release();
                }
            });
            await Task.WhenAll(tasks);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Git status refresh pass failed");
        }
    }

    /// <summary>
    /// Probes one repo and pushes the parsed result onto its entity. Any failure
    /// (missing repo, git error, timeout) still marks the repo loaded with zeroed
    /// counts so the card shows zeros instead of spinning "checking…" forever.
    /// </summary>
    private async Task RefreshRepoAsync(Repo repo, CancellationToken cancellationToken)
    {
        try
        {
            var output = await RunGitAsync(
                repo.FolderPath!,
                "--no-optional-locks status --porcelain=v2 --branch --untracked-files=all",
                cancellationToken);

            var status = ParsePorcelain(output);
            repo.GitBranchName = status.BranchName;
            repo.GitModifiedCount = status.ModifiedCount;
            repo.GitToPushCount = status.AheadCount;
            repo.GitToPullCount = status.BehindCount;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Git status failed for {FolderPath}", repo.FolderPath);
            repo.GitBranchName = null;
            repo.GitModifiedCount = 0;
            repo.GitToPushCount = 0;
            repo.GitToPullCount = 0;
        }
        finally
        {
            repo.GitStatusLoaded = true;
        }
    }

    /// <summary>
    /// Parses <c>git status --porcelain=v2 --branch --untracked-files=all</c> output.
    /// Header lines look like <c># branch.head main</c> and <c># branch.ab +2 -1</c>
    /// (the latter only when an upstream is configured); every remaining line is one
    /// change entry (ordinary, renamed, unmerged or untracked). A null/empty output
    /// yields a zeroed snapshot.
    /// </summary>
    private static GitStatusSnapshot ParsePorcelain(string? output)
    {
        string? branch = null;
        var modified = 0;
        var ahead = 0;
        var behind = 0;

        if (!string.IsNullOrEmpty(output))
        {
            foreach (var rawLine in output.Split('\n', StringSplitOptions.RemoveEmptyEntries))
            {
                var line = rawLine.TrimEnd('\r');
                if (line.StartsWith("# branch.head ", StringComparison.Ordinal))
                {
                    branch = line["# branch.head ".Length..].Trim();
                }
                else if (line.StartsWith("# branch.ab ", StringComparison.Ordinal))
                {
                    foreach (var part in line["# branch.ab ".Length..]
                                 .Split(' ', StringSplitOptions.RemoveEmptyEntries))
                    {
                        if (part.StartsWith('+'))
                            int.TryParse(part.AsSpan(1), out ahead);
                        else if (part.StartsWith('-'))
                            int.TryParse(part.AsSpan(1), out behind);
                    }
                }
                else if (!line.StartsWith('#'))
                {
                    modified++;
                }
            }
        }

        return new GitStatusSnapshot(branch, modified, ahead, behind);
    }

    /// <summary>
    /// Runs <c>git</c> with the given arguments in <paramref name="workingDir"/> and
    /// returns stdout, or <see langword="null"/> on any failure (non-zero exit, timeout,
    /// missing binary). Prompts are disabled (<c>GIT_TERMINAL_PROMPT=0</c>) and locks are
    /// not taken (<c>--no-optional-locks</c>) so probing never interferes with the user's
    /// own git operations.
    /// </summary>
    private async Task<string?> RunGitAsync(string workingDir, string arguments, CancellationToken cancellationToken)
    {
        var startInfo = new ProcessStartInfo
        {
            FileName = "git",
            Arguments = arguments,
            WorkingDirectory = workingDir,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        // Never block on credential/passphrase prompts — fail fast instead.
        startInfo.Environment["GIT_TERMINAL_PROMPT"] = "0";

        using var process = new Process { StartInfo = startInfo };
        try
        {
            if (!process.Start())
                return null;
        }
        catch (Win32Exception ex)
        {
            // git is not installed / not on PATH: disable the service until the next
            // app run instead of failing every repo on every refresh.
            _gitUnavailable = true;
            Log.Logger.Debug(ex, "git executable not found; git status checks disabled");
            return null;
        }

        using var timeoutCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutCts.CancelAfter(ProcessTimeout);

        try
        {
            var stdoutTask = process.StandardOutput.ReadToEndAsync(timeoutCts.Token);
            var stderrTask = process.StandardError.ReadToEndAsync(timeoutCts.Token);
            await process.WaitForExitAsync(timeoutCts.Token);
            await Task.WhenAll(stdoutTask, stderrTask);
            return process.ExitCode == 0 ? stdoutTask.Result : null;
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            // Timeout, not an external cancel: kill the stray git process and move on.
            try { process.Kill(entireProcessTree: true); } catch { /* already exited */ }
            return null;
        }
    }

    /// <summary>The parsed result of one repo's git status probe.</summary>
    private sealed record GitStatusSnapshot(string? BranchName, int ModifiedCount, int AheadCount, int BehindCount);
}
