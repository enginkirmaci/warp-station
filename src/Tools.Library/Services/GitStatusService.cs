using System.Collections.Concurrent;
using System.ComponentModel;
using System.Text.RegularExpressions;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Default <see cref="IGitStatusService"/>. Probes every repo with one in-process
/// libgit2 status (bounded parallelism; see <see cref="GitReadService"/>) — branch,
/// change count, ahead/behind and the last commit date land on the <see cref="Repo"/>
/// entities from background threads — CommunityToolkit raises <c>PropertyChanged</c>
/// and the bound cards update without the page VM being involved. A full pass probes
/// every repo first and applies the snapshots in one burst afterwards, so the list
/// takes a single update instead of a per-repo trickle.
/// <para>
/// Local reads run in-process; only mutations, syncs and clones shell out to the git
/// CLI through <see cref="GitCommandRunner"/>, where hooks, gpg signing and credential
/// helpers keep applying. This class owns the orchestration — the coalesced refresh
/// loop, the probe/push cadence and the Changes tab's shared snapshot probe.
/// </para>
/// <para>
/// A refresh is kicked automatically when <see cref="IRepoService"/> raises
/// <c>Changed</c> outside of a scan, so statuses re-check after every rescan without the
/// page having to coordinate anything.
/// </para>
/// </summary>
public sealed class GitStatusService : IGitStatusService
{
    /// <summary>How many repos are probed concurrently; keeps read bursts off the UI machine.</summary>
    private const int MaxParallelism = 4;

    private readonly IRepoService _repoService;
    private readonly GitCommandRunner _runner = new();
    private readonly GitReadService _reads = new();

    /// <summary>
    /// Owns the coalescing refresh loop and the throttled pass over every repo; this
    /// service only supplies the disabled guard and the per-repo probe.
    /// </summary>
    private readonly RefreshCoalescer _coalescer;

    /// <summary>
    /// In-flight change-probe runs per folder, shared by the Changes tab's two
    /// concurrent loads (file list + staged/unstaged split) so one tab load runs one
    /// shared read, not two. Entries remove themselves the moment the probes
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
        // No availability gate: the in-process reads need no git binary. Only the CLI
        // operations (mutations, syncs, clones) fail per call when git is missing.
        await _coalescer.RunCoalescedAsync(RefreshCoreAsync, cancellationToken);
    }

    /// <inheritdoc/>
    public void Stop() => _runner.Stop();

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
    /// Fetches one repo's status without touching its entity: the branch, counts and
    /// last-commit date land in the returned probe; <see cref="ApplyProbe"/> does the
    /// writing. The read itself never throws — an unreadable repository settles as a
    /// zeroed snapshot, same as a failed CLI probe did.
    /// </summary>
    private async Task<RepoStatusProbe> ProbeRepoAsync(Repo repo, CancellationToken cancellationToken)
    {
        try
        {
            var snapshot = await _reads.ProbeAsync(repo.FolderPath!);

            return new RepoStatusProbe(
                repo,
                snapshot?.BranchName,
                snapshot?.ModifiedCount ?? 0,
                snapshot?.AheadCount ?? 0,
                snapshot?.BehindCount ?? 0,
                snapshot?.LastCommitAt,
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
        // Newer-wins rather than only-if-null: rescans carry the previous entity's
        // fetch time over, so a FETCH_HEAD touched by an external fetch since then
        // must still win over the older carried value.
        if (probe.LastFetchAt is { } fetchAt && (repo.GitLastFetchAt is null || fetchAt > repo.GitLastFetchAt))
        {
            repo.GitLastFetchAt = fetchAt;
        }
        repo.GitStatusLoaded = true;
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<GitBranchRef>> GetBranchesAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return Array.Empty<GitBranchRef>();

        // Local branches first, then remote-tracking ones; the read layer keeps the
        // same shape the %(refname) parse produced (locals and remotes told apart, a
        // local branch may itself contain slashes, HEAD aliases dropped).
        return await _reads.BranchesAsync(repo.FolderPath) ?? Array.Empty<GitBranchRef>();
    }

    /// <inheritdoc/>
    public async Task<bool> CheckoutAsync(Repo repo, string branch, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(branch)) return false;

        return await RunAndRefreshAsync(repo, $"checkout {GitCommandRunner.Quote(branch)}", cancellationToken);
    }

    /// <inheritdoc/>
    public async Task<GitSyncResult> CreateBranchAsync(
        Repo repo,
        string branch,
        string? startPoint = null,
        bool checkout = true,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(branch))
        {
            return new GitSyncResult(false, null);
        }

        var quoted = GitCommandRunner.Quote(branch);
        var command = checkout ? $"checkout -b {quoted}" : $"branch {quoted}";
        if (!string.IsNullOrWhiteSpace(startPoint))
        {
            command += $" {GitCommandRunner.Quote(startPoint.Trim())}";
        }

        // Branch creation fails most often on a duplicate/invalid name — exactly the
        // failures the drawer wants to show inline, so git's fatal line is captured.
        var stderr = new List<string>();
        var ok = await RunAndRefreshAsync(repo, command, cancellationToken, stderrSink: stderr);
        return ok ? GitSyncResult.Ok() : new GitSyncResult(false, GitOutputParser.SummarizeSyncError(stderr));
    }

    /// <inheritdoc/>
    public async Task<GitSyncResult> CloneAsync(
        string url,
        string parentDirectory,
        string repoName,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(url) || string.IsNullOrWhiteSpace(parentDirectory)
            || string.IsNullOrWhiteSpace(repoName))
        {
            return new GitSyncResult(false, null);
        }

        // Clone fails most often on an unreachable URL or an existing destination —
        // exactly the failures the drawer wants to show inline, so git's fatal line is
        // captured. The working directory is the destination's PARENT: this is the one
        // command that runs outside any existing repo, and the transfer may legitimately
        // run for minutes, so it gets the longest bound.
        var stderr = new List<string>();
        var output = await RunGitAsync(
            parentDirectory,
            $"clone {GitCommandRunner.Quote(url.Trim())} {GitCommandRunner.Quote(repoName.Trim())}",
            cancellationToken,
            GitCommandRunner.CloneTimeout,
            stderr);

        if (cancellationToken.IsCancellationRequested)
        {
            // The cancelled (tree-killed) clone leaves a partial worktree behind; remove
            // it so a retried clone finds its target slot free again. Best effort:
            // Windows can still hold file locks for a beat after the kill.
            try { Directory.Delete(Path.Combine(parentDirectory, repoName), recursive: true); }
            catch { /* partial folder cleanup is best effort */ }
            return new GitSyncResult(false, null, Cancelled: true);
        }

        return output is not null ? GitSyncResult.Ok() : new GitSyncResult(false, GitOutputParser.SummarizeSyncError(stderr));
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
    /// captured and summarized into the result. Transfers can legitimately run far
    /// longer than a local mutation, so syncs get their own longer bound.
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

        return await _reads.CommitDetailsAsync(repo.FolderPath, hash)
               ?? new GitCommitDetails(hash, Array.Empty<GitChangedFile>());
    }

    /// <inheritdoc/>
    public async Task<string?> GetCommitFilePatchAsync(Repo repo, string hash, string path, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(hash) || string.IsNullOrWhiteSpace(path))
        {
            return null;
        }

        // The read layer caps the patch for the drawer's fixed-height box the same way
        // the capped CLI read did.
        return await _reads.CommitFilePatchAsync(repo.FolderPath, hash, path);
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

        // Reachability from a fetched remote-tracking ref (= pushed). The read fails
        // open on errors — the web link hides only on a positive "no remote ref
        // contains it". Only as fresh as the last fetch, like the CLI form was.
        return await _reads.CommitPushedAsync(repo.FolderPath, hash);
    }

    /// <inheritdoc/>
    public async Task<IReadOnlySet<string>?> GetUnpushedCommitHashesAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return null;

        // The batched form of IsCommitPushedAsync's "contained in some remote ref"
        // semantics, one walk for the whole History list. Null when the indicator
        // cannot be answered honestly (no remote, unreadable repository): the caller
        // hides the indicator rather than guess.
        return await _reads.UnpushedHashesAsync(repo.FolderPath);
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
    /// Runs the shared change read the Changes tab needs once — status, worktree
    /// numstat, staged numstat — and shapes the status entries into the two views
    /// (<see cref="GitChangedFile"/> list, staged/unstaged split). An empty status
    /// short-circuits to the empty snapshot without running the diff probes, exactly
    /// like the CLI version skipped them on empty porcelain output.
    /// </summary>
    private async Task<GitChangeSnapshot> RunChangeProbesAsync(string folderPath)
        => await _reads.ChangeSnapshotAsync(folderPath) ?? GitChangeSnapshot.Empty;

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
    public Task<bool> DiscardUnstagedAsync(Repo repo, IReadOnlyList<string> paths, CancellationToken cancellationToken = default)
        => DiscardPathsAsync(repo, paths, resetFirst: false, cancellationToken);

    /// <inheritdoc/>
    public Task<bool> DiscardStagedAsync(Repo repo, IReadOnlyList<string> paths, CancellationToken cancellationToken = default)
        => DiscardPathsAsync(repo, paths, resetFirst: true, cancellationToken);

    /// <summary>
    /// Shared tail of the section discards: with <paramref name="resetFirst"/> the
    /// paths are unstaged first (a staged discard's files return to HEAD), then every
    /// path still in the index is checked out (worktree back to it) and every path
    /// that is not — untracked files and just-unstaged additions — is deleted. Each
    /// command is one batched git call; the last one refreshes the repo's status.
    /// </summary>
    private async Task<bool> DiscardPathsAsync(
        Repo repo,
        IReadOnlyList<string> paths,
        bool resetFirst,
        CancellationToken cancellationToken)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || paths.Count == 0) return false;

        var quotedAll = string.Join(" ", paths.Select(GitCommandRunner.Quote));
        if (resetFirst && !await RunAndRefreshAsync(repo, $"reset -q HEAD -- {quotedAll}", cancellationToken))
        {
            return false;
        }

        // Split by index membership: exactly the paths the index knows (the in-process
        // form of the ls-files probe; a failed read lists nothing, which routes every
        // path to the untracked side, same as a failed ls-files did).
        var inIndex = await _reads.IndexedPathsAsync(repo.FolderPath!, paths)
                      ?? new HashSet<string>(StringComparer.Ordinal);

        var tracked = paths.Where(inIndex.Contains).ToList();
        var untracked = paths.Where(p => !inIndex.Contains(p)).ToList();

        if (tracked.Count > 0)
        {
            var quotedTracked = string.Join(" ", tracked.Select(GitCommandRunner.Quote));
            if (!await RunAndRefreshAsync(repo, $"checkout -- {quotedTracked}", cancellationToken))
            {
                return false;
            }
        }

        if (untracked.Count > 0)
        {
            var quotedUntracked = string.Join(" ", untracked.Select(GitCommandRunner.Quote));
            if (!await RunAndRefreshAsync(repo, $"clean -f -- {quotedUntracked}", cancellationToken))
            {
                return false;
            }
        }

        return true;
    }

    /// <inheritdoc/>
    public async Task<bool> DiscardFileAsync(Repo repo, string path, bool isStaged, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath) || string.IsNullOrWhiteSpace(path)) return false;

        var quoted = GitCommandRunner.Quote(path);

        // A staged row loses its staged state too, so the file ends at HEAD; an
        // unstaged row keeps the index (a partially staged file keeps its edits).
        if (isStaged && !await RunAndRefreshAsync(repo, $"reset -q HEAD -- {quoted}", cancellationToken))
        {
            return false;
        }

        // In the index → revert the working tree to it; not in the index → the file
        // is untracked (or was a staged addition just unstaged above): delete it.
        var inIndex = await _reads.IsTrackedAsync(repo.FolderPath!, path);

        return await RunAndRefreshAsync(
            repo,
            inIndex ? $"checkout -- {quoted}" : $"clean -f -- {quoted}",
            cancellationToken);
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
    public Task<string?> GetStagedPatchAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return Task.FromResult<string?>(null);

        // The read layer caps the patch head (the only consumer, the commit-message
        // prompt, truncates far below it anyway).
        return _reads.StagedPatchAsync(repo.FolderPath);
    }

    /// <inheritdoc/>
    public async Task<IReadOnlyList<GitCommitInfo>> GetRecentCommitsAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(repo.FolderPath)) return Array.Empty<GitCommitInfo>();

        return await _reads.RecentCommitsAsync(repo.FolderPath) ?? Array.Empty<GitCommitInfo>();
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
