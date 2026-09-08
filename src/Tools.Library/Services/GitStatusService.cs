using System.Collections.Concurrent;
using System.ComponentModel;
using System.Globalization;
using System.Text.RegularExpressions;
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

    /// <summary>
    /// Upper bound for pull/push: network transfers grow with the payload, unlike the
    /// local probes the 10 s bound is tuned for.
    /// </summary>
    private static readonly TimeSpan SyncTimeout = TimeSpan.FromSeconds(60);

    /// <summary>How many repos are probed concurrently; keeps process storms off the UI machine.</summary>
    private const int MaxParallelism = 4;

    /// <summary>
    /// Environment for every git child: never block on credential/passphrase prompts —
    /// fail fast instead.
    /// </summary>
    private static readonly IReadOnlyDictionary<string, string> GitEnvironment = new Dictionary<string, string>
    {
        ["GIT_TERMINAL_PROMPT"] = "0",
    };

    private readonly IRepoService _repoService;

    /// <summary>
    /// Owns the coalescing refresh loop and the throttled pass over every repo; this
    /// service only supplies the disabled guard and the per-repo probe.
    /// </summary>
    private readonly RefreshCoalescer _coalescer;

    /// <summary>Set once <c>git</c> is missing on PATH; subsequent refreshes become no-ops.</summary>
    private volatile bool _gitUnavailable;

    /// <summary>
    /// In-flight change-probe runs per folder, shared by the Changes tab's two
    /// concurrent loads (file list + staged/unstaged split) so one tab load spawns one
    /// set of git processes, not two. Entries remove themselves the moment the probes
    /// settle — nothing is cached, so a later load (after a stage/unstage, say) always
    /// re-probes and a git failure is never sticky.
    /// </summary>
    private readonly ConcurrentDictionary<string, Task<GitChangeSnapshot>> _inFlightChangeProbes = new(StringComparer.Ordinal);

    public GitStatusService(IRepoService repoService)
    {
        _repoService = repoService;
        _coalescer = new RefreshCoalescer(repoService);
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
        await _coalescer.RunCoalescedAsync(RefreshCoreAsync, cancellationToken);
    }

    /// <summary>
    /// One throttled refresh pass over every known repo. Per-repo failures never break
    /// the pass: <see cref="RefreshRepoAsync"/> settles a failing repo to its zeroed
    /// state, and anything still escaping is logged and swallowed here.
    /// </summary>
    private async Task RefreshCoreAsync(CancellationToken cancellationToken)
    {
        try
        {
            await _coalescer.RunThrottledPassAsync(MaxParallelism, RefreshRepoAsync, cancellationToken);
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

        return await RunAndRefreshAsync(repo, $"checkout {Quote(branch)}", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<bool> FetchAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return false;

        // The timestamp is stamped before the refresh: SeedLastFetchTime only fills a
        // null GitLastFetchAt, so the app's own fetch time must be in place first.
        return await RunAndRefreshAsync(
            repo,
            "fetch --prune",
            cancellationToken,
            onSucceeded: () => repo.GitLastFetchAt = DateTimeOffset.Now);
    }

    /// <inheritdoc/>
    public async Task<GitSyncResult> PullAsync(Repo repo, CancellationToken cancellationToken = default)
        => await SyncAsync(repo, "pull", cancellationToken);

    /// <inheritdoc/>
    public async Task<GitSyncResult> PushAsync(Repo repo, CancellationToken cancellationToken = default)
        => await SyncAsync(repo, "push", cancellationToken);

    /// <summary>
    /// Runs one network sync command (pull/push) in the repo and refreshes its status
    /// on success. Unlike the local git calls, a sync's failure reason matters to the
    /// user (no upstream, rejected non-fast-forward, conflicts…), so stderr is captured
    /// and summarized into the result. Transfers can legitimately run past the 10 s
    /// probe timeout, so syncs get their own longer bound.
    /// </summary>
    private async Task<GitSyncResult> SyncAsync(Repo repo, string command, CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return new GitSyncResult(false, null);

        var stderr = new List<string>();
        return await RunAndRefreshAsync(repo, command, cancellationToken, SyncTimeout, stderr)
            ? GitSyncResult.Ok()
            : new GitSyncResult(false, SummarizeSyncError(stderr));
    }

    /// <summary>
    /// Picks the actionable line from git's stderr for a sync failure — the first
    /// fatal/error/conflict/rejection line, falling back to the last non-empty line
    /// (transfer chatter like "From origin" would otherwise fill the notification).
    /// </summary>
    private static string? SummarizeSyncError(List<string> lines)
    {
        string? fallback = null;
        foreach (var raw in lines)
        {
            var line = raw.Trim();
            if (line.Length == 0) continue;
            if (line.StartsWith("fatal: ", StringComparison.Ordinal)
                || line.StartsWith("error: ", StringComparison.Ordinal)
                || line.StartsWith("CONFLICT", StringComparison.Ordinal)
                || line.StartsWith("! [", StringComparison.Ordinal))
            {
                return line;
            }
            fallback = line;
        }
        return fallback;
    }

    /// <inheritdoc/>
    public async Task<GitCommitDetails> GetCommitDetailsAsync(Repo repo, string hash, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(hash))
        {
            return new GitCommitDetails(hash ?? string.Empty, Array.Empty<GitChangedFile>());
        }

        // "--format=" drops the commit header, leaving just "added\tdeleted\tpath"
        // numstat lines (a rename's path renders as "old => new" — ParseNumstat takes
        // the resolved tail, which is also the path the patch command needs).
        var output = await RunGitAsync(repo.FolderPath, $"show --numstat --format= {Quote(hash)}", cancellationToken);
        var counts = ParseNumstat(output);
        var files = counts
            .Select(entry => new GitChangedFile(entry.Key, string.Empty, entry.Value.Additions, entry.Value.Deletions))
            .ToList();
        return new GitCommitDetails(hash, files);
    }

    /// <inheritdoc/>
    public async Task<string?> GetCommitFilePatchAsync(Repo repo, string hash, string path, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(hash) || string.IsNullOrWhiteSpace(path))
        {
            return null;
        }

        return await RunGitAsync(
            repo.FolderPath,
            $"show --format= {Quote(hash)} -- {Quote(path)}",
            cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<bool> RevertCommitAsync(Repo repo, string hash, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(hash)) return false;

        return await RunAndRefreshAsync(repo, $"revert --no-edit {Quote(hash)}", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<GitChangedFile>> GetChangedFilesAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return Array.Empty<GitChangedFile>();

        var snapshot = await GetChangeSnapshotAsync(repo.FolderPath);

        // Merged per-file counts: the worktree diff first, the staged (cached) diff
        // layered on top, so a path present in both reports its staged counts.
        // Untracked files appear in neither (and binary files report "-"), so those
        // stay null; a rename's numstat path ("old => new" forms) is matched by its
        // trailing path segment.
        var counts = new Dictionary<string, (int? Additions, int? Deletions)>(snapshot.WorktreeCounts);
        foreach (var (path, count) in snapshot.CachedCounts)
        {
            counts[path] = count;
        }

        var files = snapshot.StatusFiles.ToList();
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

    /// <inheritdoc/>
    public async Task<GitChangeGroups> GetChangeGroupsAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return new GitChangeGroups([], []);

        var snapshot = await GetChangeSnapshotAsync(repo.FolderPath);

        // Per-side numstat: the unstaged counts come from the worktree diff, the
        // staged ones from the cached diff. Untracked files appear in neither (and
        // binary files report "-", parsing to nulls) so they stay count-less.
        var unstaged = snapshot.UnstagedFiles
            .Select(file => snapshot.WorktreeCounts.TryGetValue(file.Path, out var addDelete)
                ? file with { Additions = addDelete.Additions, Deletions = addDelete.Deletions }
                : file)
            .ToList();

        var staged = snapshot.StagedFiles
            .Select(file => snapshot.CachedCounts.TryGetValue(file.Path, out var addDelete)
                ? file with { Additions = addDelete.Additions, Deletions = addDelete.Deletions }
                : file)
            .ToList();

        return new GitChangeGroups(staged, unstaged);
    }

    /// <summary>
    /// The shared probe run behind <see cref="GetChangedFilesAsync"/> and
    /// <see cref="GetChangeGroupsAsync"/>: the tab loads both views at once (fire-and-
    /// forget tasks on the VM side), so the two calls join the same in-flight run per
    /// folder instead of each spawning the same three git processes. The run carries no
    /// cancellation token — every current caller passes <see cref="CancellationToken.None"/>,
    /// and a shared run must not die because one joiner went away. Completed or failed
    /// runs remove themselves (see <see cref="StartChangeProbesAsync"/>), so this only
    /// ever coalesces truly concurrent loads.
    /// </summary>
    private Task<GitChangeSnapshot> GetChangeSnapshotAsync(string folderPath)
        => _inFlightChangeProbes.GetOrAdd(folderPath, StartChangeProbesAsync);

    /// <summary>
    /// Starts one probe run and arms its self-removal: the conditional
    /// <c>TryRemove</c> fires on completion and fault alike, but only while the entry
    /// still holds this very task — a newer run for the same folder is left alone. The
    /// next load therefore always starts from fresh probes and a failed run is never
    /// served twice.
    /// </summary>
    private Task<GitChangeSnapshot> StartChangeProbesAsync(string folderPath)
    {
        var probes = RunChangeProbesAsync(folderPath);
        probes.ContinueWith(
            (task, state) => _inFlightChangeProbes.TryRemove(
                new KeyValuePair<string, Task<GitChangeSnapshot>>((string)state!, task)),
            folderPath,
            CancellationToken.None,
            TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);
        return probes;
    }

    /// <summary>
    /// Runs the three git probes the Changes tab needs once — status (porcelain v2),
    /// worktree numstat, staged numstat — and parses the status output into the two
    /// views (<see cref="GitChangedFile"/> list, staged/unstaged split). An empty status
    /// (clean repo, or git failing outright) short-circuits to the empty snapshot
    /// without spawning the diff probes, exactly like the pre-merge code.
    /// </summary>
    private async Task<GitChangeSnapshot> RunChangeProbesAsync(string folderPath)
    {
        var statusOutput = await RunGitAsync(
            folderPath,
            "--no-optional-locks status --porcelain=v2 --untracked-files=all",
            CancellationToken.None);
        if (string.IsNullOrEmpty(statusOutput)) return EmptyChangeSnapshot;

        var statusFiles = new List<GitChangedFile>();
        var staged = new List<GitChangedFile>();
        var unstaged = new List<GitChangedFile>();
        foreach (var rawLine in statusOutput.Split('\n', StringSplitOptions.RemoveEmptyEntries))
        {
            var line = rawLine.TrimEnd('\r');
            if (line.Length == 0 || line[0] == '#') continue;
            ParseStatusLine(line, statusFiles, staged, unstaged);
        }

        return new GitChangeSnapshot(
            statusFiles,
            staged,
            unstaged,
            ParseNumstat(await RunGitAsync(folderPath, "--no-optional-locks diff --numstat", CancellationToken.None)),
            ParseNumstat(await RunGitAsync(folderPath, "--no-optional-locks diff --cached --numstat", CancellationToken.None)));
    }

    /// <summary>
    /// Parses one porcelain v2 change line into the two views the Changes tab shows.
    /// Ordinary/renamed entries (<c>1</c>/<c>2</c>): the XY pair right after the kind —
    /// X is the index (staged) status, Y the worktree (unstaged) status, '.' meaning
    /// unmodified on that side; the path is the LAST space-separated token (a rename's
    /// original path precedes it). The file list keeps the whole XY pair verbatim and
    /// only non-empty paths; the split view emits one entry per non-'.' side. Unmerged
    /// entries (<c>u</c>) surface as a "U" worktree entry — there is no clean index
    /// side to stage until the conflict is resolved — while the file list reads the XY
    /// pair like any ordinary line. Untracked entries (<c>?</c>): the whole remainder
    /// IS the path (it may contain spaces), worktree side only.
    /// </summary>
    private static void ParseStatusLine(
        string line,
        List<GitChangedFile> statusFiles,
        List<GitChangedFile> staged,
        List<GitChangedFile> unstaged)
    {
        var separator = line.IndexOf(' ');
        if (separator <= 0) return;

        var kind = line[..separator];
        var rest = line[(separator + 1)..];

        if (kind == "?")
        {
            if (rest.Length > 0) statusFiles.Add(new GitChangedFile(rest, "?"));
            unstaged.Add(new GitChangedFile(rest, "?"));
            return;
        }

        if (kind == "u")
        {
            unstaged.Add(new GitChangedFile(LastToken(rest), "U"));

            var unmergedCodeEnd = rest.IndexOf(' ');
            if (unmergedCodeEnd <= 0) return;
            var unmergedPath = LastToken(rest[(unmergedCodeEnd + 1)..]);
            if (unmergedPath.Length > 0)
            {
                statusFiles.Add(new GitChangedFile(unmergedPath, rest[..unmergedCodeEnd]));
            }
            return;
        }

        if (rest.Length < 3) return;
        var indexCode = rest[0];
        var worktreeCode = rest[1];
        var path = LastToken(rest[3..]);

        if (indexCode is not ('.' or ' '))
        {
            staged.Add(new GitChangedFile(path, indexCode.ToString()));
        }
        if (worktreeCode is not ('.' or ' '))
        {
            unstaged.Add(new GitChangedFile(path, worktreeCode.ToString()));
        }
        if (path.Length > 0)
        {
            // (Path, StatusCode) — the flat file list shows the path and the verbatim
            // XY pair as its status.
            statusFiles.Add(new GitChangedFile(path, rest[..2]));
        }
    }

    /// <summary>
    /// One repo's parsed change probes: the status lines already shaped for the two
    /// views plus the raw per-side numstat dictionaries, which the views apply with
    /// different merge rules (the file list layers the cached counts over the worktree
    /// counts; the split view keeps them per side). Instances are shared between the
    /// two concurrent views — the projections only read the snapshot and build their
    /// own lists on top.
    /// </summary>
    private sealed record GitChangeSnapshot(
        IReadOnlyList<GitChangedFile> StatusFiles,
        IReadOnlyList<GitChangedFile> StagedFiles,
        IReadOnlyList<GitChangedFile> UnstagedFiles,
        Dictionary<string, (int? Additions, int? Deletions)> WorktreeCounts,
        Dictionary<string, (int? Additions, int? Deletions)> CachedCounts);

    /// <summary>The shared empty snapshot for an empty status output.</summary>
    private static readonly GitChangeSnapshot EmptyChangeSnapshot = new(
        [],
        [],
        [],
        new Dictionary<string, (int?, int?)>(StringComparer.Ordinal),
        new Dictionary<string, (int?, int?)>(StringComparer.Ordinal));

    /// <summary>The last space-separated token of a porcelain v2 change line.</summary>
    private static string LastToken(string fields)
    {
        var lastSpace = fields.LastIndexOf(' ');
        return lastSpace >= 0 ? fields[(lastSpace + 1)..] : fields;
    }

    /// <inheritdoc/>
    public async Task<bool> StageAsync(Repo repo, string path, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(path)) return false;

        return await RunAndRefreshAsync(repo, $"add -- {Quote(path)}", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<bool> StageAllAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return false;

        return await RunAndRefreshAsync(repo, "add -A", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<bool> UnstageAsync(Repo repo, string path, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(path)) return false;

        return await RunAndRefreshAsync(repo, $"reset -q HEAD -- {Quote(path)}", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<bool> UnstageAllAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return false;

        return await RunAndRefreshAsync(repo, "reset -q HEAD", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<string?> CommitAsync(Repo repo, string message, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(message)) return null;

        // The message goes through a temp file (-F) instead of -m: it may carry
        // quotes, newlines or anything else a shell-style argument would mangle.
        var messageFile = Path.Combine(Path.GetTempPath(), $"devtools-commit-{Guid.NewGuid():N}.msg");
        try
        {
            await File.WriteAllTextAsync(messageFile, message, cancellationToken);
            var output = await RunGitAsync(repo.FolderPath, $"commit -F {Quote(messageFile)}", cancellationToken);
            if (output is null) return null;

            // First line of git's output: "[branch abc1234] subject" — pick the hash.
            var match = Regex.Match(output, @"\[[^\]]+?\s([0-9a-f]{7,40})\]");
            await RefreshRepoAsync(repo, cancellationToken);
            return match.Success ? match.Groups[1].Value[..7] : string.Empty;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "git commit failed for {FolderPath}", repo.FolderPath);
            return null;
        }
        finally
        {
            try { File.Delete(messageFile); } catch { /* best effort cleanup */ }
        }
    }

    /// <inheritdoc/>
    public async Task<string?> GetStagedPatchAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return null;
        return await RunGitAsync(repo.FolderPath, "--no-optional-locks diff --cached", cancellationToken);
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
            "log -10 --pretty=format:%H%x09%s%x09%an%x09%cI",
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
    private async Task<string?> RunGitAsync(
        string workingDir,
        string arguments,
        CancellationToken cancellationToken,
        TimeSpan? timeout = null,
        ICollection<string>? stderrSink = null)
    {
        ProcessRunResult result;
        try
        {
            result = await ProcessRunner.RunAsync(new ProcessRunOptions
            {
                FileName = "git",
                Arguments = arguments,
                WorkingDirectory = workingDir,
                Timeout = timeout ?? ProcessTimeout,
                EnvironmentVariables = GitEnvironment,
            }, cancellationToken);
        }
        catch (Win32Exception ex)
        {
            // git is not installed / not on PATH: disable the service until the next
            // app run instead of failing every repo on every refresh.
            _gitUnavailable = true;
            Log.Logger.Debug(ex, "git executable not found; git status checks disabled");
            return null;
        }

        // Sync failures surface their stderr to the user — same line-splitting as before,
        // so the notification still carries one actionable git line.
        if (result.ExitCode != 0 && stderrSink is not null)
        {
            foreach (var line in result.StandardError.Split('\n', StringSplitOptions.RemoveEmptyEntries))
            {
                stderrSink.Add(line);
            }
        }

        return result.ExitCode == 0 ? result.StandardOutput : null;
    }

    /// <summary>
    /// Runs one mutating git command and, when it succeeded, refreshes the repo's status
    /// before reporting success — the shape every state-changing command below repeats.
    /// <paramref name="timeout"/> and <paramref name="stderrSink"/> pass through to
    /// <see cref="RunGitAsync"/> (the sync commands' longer bound and stderr capture);
    /// <paramref name="onSucceeded"/> runs after the command but before the refresh, so
    /// <see cref="FetchAsync"/> can stamp <see cref="Repo.GitLastFetchAt"/> ahead of it.
    /// </summary>
    private async Task<bool> RunAndRefreshAsync(
        Repo repo,
        string arguments,
        CancellationToken cancellationToken,
        TimeSpan? timeout = null,
        ICollection<string>? stderrSink = null,
        Action? onSucceeded = null)
    {
        var ok = await RunGitAsync(repo.FolderPath!, arguments, cancellationToken, timeout, stderrSink) is not null;
        if (ok)
        {
            onSucceeded?.Invoke();
            await RefreshRepoAsync(repo, cancellationToken);
        }

        return ok;
    }

    /// <summary>The parsed result of one repo's git status probe.</summary>
    private sealed record GitStatusSnapshot(string? BranchName, int ModifiedCount, int AheadCount, int BehindCount);
}
