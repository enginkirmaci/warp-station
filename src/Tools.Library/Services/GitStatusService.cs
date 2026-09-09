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
/// cards update without the page VM being involved. A full pass probes every repo
/// first and applies the snapshots in one burst afterwards, so the list takes a
/// single update instead of a per-repo trickle.
/// <para>
/// The process mechanics live in <see cref="GitCommandRunner"/> and the output parsing
/// in <see cref="GitOutputParser"/>; this class owns the orchestration — the coalesced
/// refresh loop, the probe/push cadence and the Changes tab's shared snapshot probe.
/// </para>
/// <para>
/// A refresh is kicked automatically when <see cref="IRepoService"/> raises
/// <c>Changed</c> outside of a scan, so statuses re-check after every rescan without the
/// page having to coordinate anything.
/// </para>
/// </summary>
public sealed class GitStatusService : IGitStatusService
{
    /// <summary>How many repos are probed concurrently; keeps process storms off the UI machine.</summary>
    private const int MaxParallelism = 4;

    private readonly IRepoService _repoService;
    private readonly GitCommandRunner _runner = new();

    /// <summary>
    /// Owns the coalescing refresh loop and the throttled pass over every repo; this
    /// service only supplies the disabled guard and the per-repo probe.
    /// </summary>
    private readonly RefreshCoalescer _coalescer;

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
        if (_runner.IsUnavailable) return;
        await _coalescer.RunCoalescedAsync(RefreshCoreAsync, cancellationToken);
    }

    /// <summary>
    /// One throttled refresh pass over every known repo. Probes run concurrently and
    /// only collect snapshots — once every probe has settled, the batch is applied in
    /// one synchronous burst so the bound cards take a single update instead of a
    /// per-repo trickle. Per-repo failures never break the pass: a failing probe
    /// settles as zeroed, and anything still escaping is logged and swallowed here.
    /// </summary>
    private async Task RefreshCoreAsync(CancellationToken cancellationToken)
    {
        try
        {
            var probes = new ConcurrentBag<RepoStatusProbe>();
            await _coalescer.RunThrottledPassAsync(
                MaxParallelism,
                async (repo, token) => probes.Add(await ProbeRepoAsync(repo, token)),
                cancellationToken);

            foreach (var probe in probes)
            {
                ApplyProbe(probe);
            }
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Git status refresh pass failed");
        }
    }

    /// <inheritdoc/>
    public async Task RefreshRepoAsync(Repo repo, CancellationToken cancellationToken)
        => ApplyProbe(await ProbeRepoAsync(repo, cancellationToken));

    /// <summary>
    /// Fetches one repo's status without touching its entity: the status porcelain
    /// parse, the last-commit date and the FETCH_HEAD seed all land in the returned
    /// snapshot; <see cref="ApplyProbe"/> does the writing. A canceled token escapes —
    /// the caller's batch is then discarded whole rather than half-applied.
    /// </summary>
    private async Task<RepoStatusProbe> ProbeRepoAsync(Repo repo, CancellationToken cancellationToken)
    {
        try
        {
            var output = await RunGitAsync(
                repo.FolderPath!,
                "--no-optional-locks status --porcelain=v2 --branch --untracked-files=all",
                cancellationToken);

            var status = GitOutputParser.ParsePorcelain(output);

            // Second, cheap local probe for the Last Activity column. Kept separate from
            // the status call so a malformed date can never blank the status fields.
            // An empty output (repo with no commits) parses to null.
            var commitDate = await RunGitAsync(
                repo.FolderPath!,
                "--no-optional-locks log -1 --format=%cI",
                cancellationToken);

            return new RepoStatusProbe(
                repo,
                status.BranchName,
                status.ModifiedCount,
                status.AheadCount,
                status.BehindCount,
                DateTimeOffset.TryParse(commitDate?.TrimEnd('\r', '\n'), out var at) ? at : null,
                ReadLastFetchSeed(repo));
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Git status failed for {FolderPath}", repo.FolderPath);
            return new RepoStatusProbe(repo, null, 0, 0, 0, null, null);
        }
    }

    /// <summary>
    /// Pushes a probe's snapshot onto its entity. Batched passes run this once per repo
    /// only after every probe has settled, so the whole list updates in one go.
    /// </summary>
    private void ApplyProbe(RepoStatusProbe probe)
    {
        var repo = probe.Repo;
        repo.GitBranchName = probe.BranchName;
        repo.GitModifiedCount = probe.ModifiedCount;
        repo.GitToPushCount = probe.AheadCount;
        repo.GitToPullCount = probe.BehindCount;
        repo.GitLastCommitAt = probe.LastCommitAt;
        if (probe.LastFetchAt is { } fetchAt && repo.GitLastFetchAt is null)
        {
            repo.GitLastFetchAt = fetchAt;
        }
        repo.GitStatusLoaded = true;
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

        return await RunAndRefreshAsync(repo, $"checkout {GitCommandRunner.Quote(branch)}", cancellationToken);
    }

    /// <inheritdoc/>
    public Task<GitSyncResult> FetchAsync(Repo repo, CancellationToken cancellationToken = default)
        => SyncAsync(
            repo,
            "fetch --prune",
            cancellationToken,
            // The timestamp is stamped before the refresh: SeedLastFetchTime only fills a
            // null GitLastFetchAt, so the app's own fetch time must be in place first.
            onSucceeded: () => repo.GitLastFetchAt = DateTimeOffset.Now);

    /// <inheritdoc/>
    public async Task<GitSyncResult> PullAsync(Repo repo, CancellationToken cancellationToken = default)
        => await SyncAsync(repo, "pull", cancellationToken);

    /// <inheritdoc/>
    public async Task<GitSyncResult> PushAsync(Repo repo, CancellationToken cancellationToken = default)
        => await SyncAsync(repo, "push", cancellationToken);

    /// <summary>
    /// Runs one network sync command (fetch/pull/push) in the repo and refreshes its
    /// status on success. Unlike the local git calls, a sync's failure reason matters to
    /// the user (no upstream, rejected non-fast-forward, conflicts…), so stderr is
    /// captured and summarized into the result. Transfers can legitimately run past the
    /// 10 s probe timeout, so syncs get their own longer bound.
    /// </summary>
    private async Task<GitSyncResult> SyncAsync(
        Repo repo,
        string command,
        CancellationToken cancellationToken,
        Action? onSucceeded = null)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return new GitSyncResult(false, null);

        var stderr = new List<string>();
        return await RunAndRefreshAsync(repo, command, cancellationToken, GitCommandRunner.SyncTimeout, stderr, onSucceeded)
            ? GitSyncResult.Ok()
            : new GitSyncResult(false, GitOutputParser.SummarizeSyncError(stderr));
    }

    /// <inheritdoc/>
    public async Task<GitCommitDetails> GetCommitDetailsAsync(Repo repo, string hash, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(hash))
        {
            return new GitCommitDetails(hash ?? string.Empty, Array.Empty<GitChangedFile>());
        }

        // "--format=" drops the commit header; --numstat already suppresses the patch
        // body (any explicit diff format does), so the output is just the
        // "added\tdeleted\tpath" lines (a rename's path renders as "old => new" —
        // ParseNumstat takes the resolved tail, which is also the path the patch
        // command needs). Verified: adding --no-patch here EMPTIES the numstat too.
        var output = await RunGitAsync(repo.FolderPath, $"show --numstat --format= {GitCommandRunner.Quote(hash)}", cancellationToken);
        var counts = GitOutputParser.ParseNumstat(output);
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

        // Capped: the patch renders in a fixed-height read-only box, and the TextBox
        // pays for every character with text layout — a single generated file's
        // multi-MB patch would spike the drawer for content nobody scrolls to.
        return await RunGitAsync(
            repo.FolderPath,
            $"show --format= {GitCommandRunner.Quote(hash)} -- {GitCommandRunner.Quote(path)}",
            cancellationToken,
            maxOutputChars: CommitPatchReadCap,
            truncationSuffix: "\n… (patch truncated)");
    }

    /// <inheritdoc/>
    public async Task<bool> RevertCommitAsync(Repo repo, string hash, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(hash)) return false;

        return await RunAndRefreshAsync(repo, $"revert --no-edit {GitCommandRunner.Quote(hash)}", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<bool> IsCommitPushedAsync(Repo repo, string hash, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(hash)) return false;

        // Remote-tracking branches containing the commit: non-empty = reachable from
        // some fetched remote ref (= pushed). Note the list is only as fresh as the
        // last fetch. A git error (null) fails open — the web link hides only on a
        // positive "no remote ref contains it".
        var output = await RunGitAsync(repo.FolderPath, $"branch -r --contains {GitCommandRunner.Quote(hash)}", cancellationToken);
        return output is null || output.Trim().Length > 0;
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
        GitOutputParser.ForEachLine(statusOutput, line =>
        {
            if (line.IsEmpty || line[0] == '#') return;
            GitOutputParser.ParseStatusLine(line, statusFiles, staged, unstaged);
        });

        return new GitChangeSnapshot(
            statusFiles,
            staged,
            unstaged,
            GitOutputParser.ParseNumstat(await RunGitAsync(folderPath, "--no-optional-locks diff --numstat", CancellationToken.None)),
            GitOutputParser.ParseNumstat(await RunGitAsync(folderPath, "--no-optional-locks diff --cached --numstat", CancellationToken.None)));
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

    /// <inheritdoc/>
    public async Task<bool> StageAsync(Repo repo, string path, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(path)) return false;

        return await RunAndRefreshAsync(repo, $"add -- {GitCommandRunner.Quote(path)}", cancellationToken);
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

        return await RunAndRefreshAsync(repo, $"reset -q HEAD -- {GitCommandRunner.Quote(path)}", cancellationToken);
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
            var output = await RunGitAsync(repo.FolderPath, $"commit -F {GitCommandRunner.Quote(messageFile)}", cancellationToken);
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

        // The only consumer (the commit-message prompt) truncates the patch to 8,000
        // chars anyway, so the read stops at a fixed head instead of materializing a
        // multi-megabyte diff (LOH churn on every wand press).
        return await RunGitAsync(
            repo.FolderPath,
            "--no-optional-locks diff --cached",
            cancellationToken,
            maxOutputChars: StagedPatchReadCap);
    }

    /// <summary>Read cap for the staged patch: comfortably past the prompt's own
    /// 8,000-char truncation point, yet far under LOH size for any real diff.</summary>
    private const int StagedPatchReadCap = 48 * 1024;

    /// <summary>Read cap for one History drawer file patch: 64K chars is ~4,000 lines —
    /// far past what the drawer's fixed-height box usefully shows, and it bounds the
    /// TextBox's text layout to match.</summary>
    private const int CommitPatchReadCap = 64 * 1024;

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
    /// Reads <c>.git/FETCH_HEAD</c>'s last write time as the seed candidate for
    /// <see cref="Repo.GitLastFetchAt"/> — a repo fetched outside the app still reports
    /// an honest age instead of "never". Null when unreadable; the entity write (and
    /// the only-if-unset guard) happens in <see cref="ApplyProbe"/>.
    /// </summary>
    private static DateTimeOffset? ReadLastFetchSeed(Repo repo)
    {
        if (repo.FolderPath is null) return null;

        try
        {
            var fetchHead = Path.Combine(repo.FolderPath, ".git", "FETCH_HEAD");
            return File.Exists(fetchHead) ? File.GetLastWriteTimeUtc(fetchHead) : null;
        }
        catch
        {
            // A missing/locked FETCH_HEAD just leaves the timestamp unset.
            return null;
        }
    }

    /// <summary>
    /// Runs one git command through the runner (stdout, or null on any failure —
    /// non-zero exit, timeout, missing binary).
    /// </summary>
    private Task<string?> RunGitAsync(
        string workingDir,
        string arguments,
        CancellationToken cancellationToken,
        TimeSpan? timeout = null,
        ICollection<string>? stderrSink = null,
        int? maxOutputChars = null,
        string? truncationSuffix = null)
        => _runner.RunAsync(workingDir, arguments, cancellationToken, timeout, stderrSink, maxOutputChars, truncationSuffix);

    /// <summary>
    /// Runs one mutating git command and, when it succeeded, refreshes the repo's status
    /// before reporting success — the shape every state-changing command below repeats.
    /// <paramref name="timeout"/> and <paramref name="stderrSink"/> pass through to
    /// <see cref="RunGitAsync"/> (the network commands' longer bound and stderr capture);
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

    /// <summary>
    /// One repo's parsed status, captured during probing and applied to the entity
    /// afterwards — a batched pass collects these for every repo first, then applies
    /// them all in one burst so the UI takes a single update. A failed probe carries
    /// zeroed values, which <see cref="ApplyProbe"/> settles the card to.
    /// </summary>
    private sealed record RepoStatusProbe(
        Repo Repo,
        string? BranchName,
        int ModifiedCount,
        int AheadCount,
        int BehindCount,
        DateTimeOffset? LastCommitAt,
        DateTimeOffset? LastFetchAt);
}
