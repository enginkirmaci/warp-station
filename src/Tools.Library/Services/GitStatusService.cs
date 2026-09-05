using System.ComponentModel;
using System.Diagnostics;
using System.Globalization;
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
/// A follow-up <c>git log -1 --format=%cI</c> picks up the last commit date for the
/// Last Activity column. Results are pushed onto the <see cref="Repo"/> entities from
/// background threads — CommunityToolkit raises <c>PropertyChanged</c> and the bound
/// cards update without the page VM being involved.
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
    public async Task RefreshRepoAsync(Repo repo, CancellationToken cancellationToken)
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

            // Second, cheap local probe for the Last Activity column. Kept separate from
            // the status call so a malformed date can never blank the status fields.
            // An empty output (repo with no commits) parses to null.
            var commitDate = await RunGitAsync(
                repo.FolderPath!,
                "--no-optional-locks log -1 --format=%cI",
                cancellationToken);
            repo.GitLastCommitAt = DateTimeOffset.TryParse(
                commitDate?.TrimEnd('\r', '\n'), out var at) ? at : null;

            SeedLastFetchTime(repo);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Git status failed for {FolderPath}", repo.FolderPath);
            repo.GitBranchName = null;
            repo.GitModifiedCount = 0;
            repo.GitToPushCount = 0;
            repo.GitToPullCount = 0;
            repo.GitLastCommitAt = null;
        }
        finally
        {
            repo.GitStatusLoaded = true;
        }
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<string>> GetBranchesAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return Array.Empty<string>();

        var output = await RunGitAsync(repo.FolderPath, "branch --format=%(refname:short)", cancellationToken);
        if (string.IsNullOrEmpty(output)) return Array.Empty<string>();

        return output
            .Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
            .Where(b => b.Length > 0)
            .ToList();
    }

    /// <inheritdoc/>
    public async Task<bool> CheckoutAsync(Repo repo, string branch, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(branch)) return false;

        var ok = await RunGitAsync(repo.FolderPath, $"checkout {Quote(branch)}", cancellationToken) is not null;
        if (ok)
        {
            await RefreshRepoAsync(repo, cancellationToken);
        }
        return ok;
    }

    /// <inheritdoc/>
    public async Task<bool> FetchAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return false;

        var ok = await RunGitAsync(repo.FolderPath, "fetch --prune", cancellationToken) is not null;
        if (ok)
        {
            repo.GitLastFetchAt = DateTimeOffset.Now;
            await RefreshRepoAsync(repo, cancellationToken);
        }
        return ok;
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<GitChangedFile>> GetChangedFilesAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return Array.Empty<GitChangedFile>();

        var output = await RunGitAsync(
            repo.FolderPath,
            "--no-optional-locks status --porcelain=v2 --untracked-files=all",
            cancellationToken);
        if (string.IsNullOrEmpty(output)) return Array.Empty<GitChangedFile>();

        var files = new List<GitChangedFile>();
        foreach (var rawLine in output.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var line = rawLine.TrimEnd('\r');
            if (line.Length == 0 || line[0] == '#') continue;

            // Ordinary/renamed/unmerged entries: "1 <XY> … <path>" / "2 <XY> … <orig> <path>" —
            // the path is always the LAST space-separated token (renames carry the original
            // path before it). Untracked entries: "? <path>".
            var separator = line.IndexOf(' ');
            if (separator <= 0) continue;

            var kind = line[..separator];
            var rest = line[(separator + 1)..];
            string statusCode;
            string path;
            if (kind == "?")
            {
                // Untracked: the whole remainder IS the path (it may contain spaces).
                statusCode = "?";
                path = rest;
            }
            else
            {
                // Ordinary/renamed/unmerged: "XY <fields…> <path>" — the path is the last
                // space-separated field (a rename's original path precedes it).
                var codeEnd = rest.IndexOf(' ');
                if (codeEnd <= 0) continue;
                statusCode = rest[..codeEnd];
                var rest2 = rest[(codeEnd + 1)..];
                var lastSpace = rest2.LastIndexOf(' ');
                path = lastSpace >= 0 ? rest2[(lastSpace + 1)..] : rest2;
            }

            if (path.Length > 0)
            {
                files.Add(new GitChangedFile(path, statusCode));
            }
        }

        // Merge in per-file added/deleted line counts from the staged + unstaged numstat
        // diffs. Untracked files never appear there (and binary files report "-"), so
        // those stay null; a rename's numstat path ("old => new" forms) is matched by its
        // trailing path segment.
        var counts = ParseNumstat(await RunGitAsync(repo.FolderPath, "--no-optional-locks diff --numstat", cancellationToken));
        foreach (var (path, count) in ParseNumstat(await RunGitAsync(repo.FolderPath, "--no-optional-locks diff --cached --numstat", cancellationToken)))
        {
            counts[path] = count;
        }
        if (counts.Count > 0)
        {
            for (var i = 0; i < files.Count; i++)
            {
                if (counts.TryGetValue(files[i].Path, out var addDelete))
                {
                    files[i] = files[i] with { Additions = addDelete.Additions, Deletions = addDelete.Deletions };
                }
            }
        }

        return files;
    }

    /// <summary>Parses <c>git diff --numstat</c> output into per-path add/delete counts.</summary>
    private static Dictionary<string, (int? Additions, int? Deletions)> ParseNumstat(string? output)
    {
        var counts = new Dictionary<string, (int?, int?)>(StringComparer.Ordinal);
        if (string.IsNullOrEmpty(output)) return counts;

        foreach (var rawLine in output.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var line = rawLine.TrimEnd('\r');
            // "additions\tdeletions\tpath" — the path may contain spaces (and renames
            // render as "old => new" or "{prefix old => suffix new}"), so split exactly
            // two tab-separated counts off the front.
            var firstTab = line.IndexOf('\t');
            if (firstTab <= 0) continue;
            var secondTab = line.IndexOf('\t', firstTab + 1);
            if (secondTab <= 0) continue;

            var path = line[(secondTab + 1)..];
            var arrow = path.LastIndexOf(" => ", StringComparison.Ordinal);
            if (arrow >= 0)
            {
                path = path[(arrow + 4)..];
            }

            int? additions = int.TryParse(line[..firstTab], out var a) ? a : null;
            int? deletions = int.TryParse(line[(firstTab + 1)..secondTab], out var d) ? d : null;
            counts[path] = (additions, deletions);
        }

        return counts;
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<GitCommitInfo>> GetRecentCommitsAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return Array.Empty<GitCommitInfo>();

        var output = await RunGitAsync(
            repo.FolderPath,
            "log -10 --pretty=format:%h%x09%s%x09%an%x09%cI",
            cancellationToken);
        if (string.IsNullOrEmpty(output)) return Array.Empty<GitCommitInfo>();

        var commits = new List<GitCommitInfo>();
        foreach (var rawLine in output.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var parts = rawLine.TrimEnd('\r').Split('\t', 4);
            if (parts.Length < 4) continue;
            if (!DateTimeOffset.TryParse(parts[3], CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var date))
            {
                continue;
            }
            commits.Add(new GitCommitInfo(parts[0], parts[1], parts[2], date));
        }

        return commits;
    }

    /// <summary>
    /// Quotes a git argument when it contains spaces (branch names rarely do, but a
    /// ref with one must not split into two arguments).
    /// </summary>
    private static string Quote(string value)
        => value.Contains(' ') ? $"\"{value}\"" : value;

    /// <summary>
    /// Seeds <see cref="Repo.GitLastFetchAt"/> from <c>.git/FETCH_HEAD</c>'s last write
    /// time when the app has not fetched itself yet — a repo fetched outside the app
    /// still reports an honest age instead of "never".
    /// </summary>
    private static void SeedLastFetchTime(Repo repo)
    {
        if (repo.GitLastFetchAt is not null || repo.FolderPath is null) return;

        try
        {
            var fetchHead = Path.Combine(repo.FolderPath, ".git", "FETCH_HEAD");
            if (File.Exists(fetchHead))
            {
                repo.GitLastFetchAt = File.GetLastWriteTimeUtc(fetchHead);
            }
        }
        catch
        {
            // A missing/locked FETCH_HEAD just leaves the timestamp unset.
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
