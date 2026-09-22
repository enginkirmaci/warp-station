using LibGit2Sharp;
using Serilog;
using System.Text.RegularExpressions;
using Tools.Library.Entities;

namespace Tools.Library.Services;

/// <summary>
/// The in-process git read layer behind <see cref="GitStatusService"/>: every local
/// read (status probes, diffs, history, branches) runs against libgit2 inside the
/// app's own process instead of spawning a fresh <c>git</c> process per command. One
/// repository handle per operation, no shared state, and no index locks taken, so
/// reads never interfere with the user's own git operations. Mutations, syncs and
/// clones stay on the CLI (see <see cref="GitCommandRunner"/>): hooks, gpg signing and
/// credential helpers must keep applying. Every failure maps to the same null/empty
/// result the CLI failure paths returned, so callers keep their failure semantics.
/// <para>
/// libgit2 is blocking and offers no cancellation: an operation runs to completion on
/// the thread pool. That is the accepted trade for removing the process spawn entirely
/// (no per-invocation cost, no antivirus re-scan per call); the app's exit always
/// reclaims everything, unlike a spawned child.
/// </para>
/// </summary>
internal sealed class GitReadService
{
    /// <summary>Read cap for the staged patch head: comfortably past the commit-message
    /// prompt's own 8,000-char truncation point, yet far under LOH size.</summary>
    internal const int StagedPatchReadCap = 48 * 1024;

    /// <summary>Read cap for one History drawer file patch: 64K chars is ~4,000 lines,
    /// far past what the drawer's fixed-height box usefully shows.</summary>
    internal const int CommitPatchReadCap = 64 * 1024;

    /// <summary>Read cap for a commit-detail message body: prose in a capped scroll
    /// box, bounded well past anything displayed.</summary>
    internal const int CommitBodyReadCap = 16 * 1024;

    /// <summary>Suffix appended when a patch read hits its cap.</summary>
    private const string PatchTruncationSuffix = "\n… (patch truncated)";

    /// <summary>Logged once, not per read, when the native libgit2 fails to load.</summary>
    private static bool _nativeFailureLogged;

    /// <summary>
    /// One repo's status probe: branch name, change count (one per file, untracked
    /// files individually), ahead/behind vs the upstream as of the last fetch, and the
    /// last commit's committer time. <paramref name="HasRemote"/> is whether any git
    /// remote is configured; <paramref name="BranchOnRemote"/> is whether the current
    /// branch has a remote-tracking counterpart (fetch/push create it) — the publish
    /// affordance shows only for a remote-configured repo whose branch is local-only.
    /// Null when the folder is not a readable repository.
    /// </summary>
    internal sealed record Probe(
        string? BranchName,
        int ModifiedCount,
        int AheadCount,
        int BehindCount,
        DateTimeOffset? LastCommitAt,
        bool HasRemote,
        bool BranchOnRemote);

    public Task<Probe?> ProbeAsync(string folderPath)
        => RunAsync(folderPath, repo =>
        {
            var status = repo.RetrieveStatus(StatusOptionsFor());
            var head = repo.Head;
            var tracking = head.TrackingDetails;
            var hasRemote = repo.Network.Remotes.Any();
            var branchOnRemote = hasRemote
                && !repo.Info.IsHeadDetached
                && repo.Branches.Any(b =>
                    b.IsRemote && b.FriendlyName.EndsWith("/" + head.FriendlyName, StringComparison.Ordinal));

            return new Probe(
                repo.Info.IsHeadDetached ? "(detached)" : head.FriendlyName,
                status.Count(),
                tracking?.AheadBy ?? 0,
                tracking?.BehindBy ?? 0,
                head.Tip?.Committer.When,
                hasRemote,
                branchOnRemote);
        });

    /// <summary>
    /// The Changes tab's shared snapshot: the status entries already shaped for the two
    /// views (flat file list, staged/unstaged split) plus the raw per-side numstat
    /// dictionaries the views apply with different merge rules. An empty status
    /// short-circuits to the empty snapshot without running the diff probes, exactly
    /// like the CLI version skipped them on empty porcelain output.
    /// </summary>
    public Task<GitChangeSnapshot?> ChangeSnapshotAsync(string folderPath)
        => RunAsync(folderPath, repo =>
        {
            var status = repo.RetrieveStatus(StatusOptionsFor());
            if (!status.Any()) return GitChangeSnapshot.Empty;

            var statusFiles = new List<GitChangedFile>();
            var staged = new List<GitChangedFile>();
            var unstaged = new List<GitChangedFile>();
            foreach (var entry in status)
            {
                DescribeStatusEntry(entry, statusFiles, staged, unstaged);
            }

            // The worktree diff is index vs worktree (git diff; the paths-only overload
            // is the index-to-workdir compare, untracked excluded like numstat), the
            // cached one is HEAD vs index (git diff --cached). One unborn-HEAD
            // divergence: git special-cases that to an empty-tree diff, libgit2 reports
            // index vs index (empty) for the staged side. Untracked files appear in
            // neither diff, matching numstat.
            return new GitChangeSnapshot(
                statusFiles,
                staged,
                unstaged,
                NumstatCounts(repo.Diff.Compare<Patch>(null, false, null, PatchOptions(repo))),
                NumstatCounts(repo.Diff.Compare<Patch>(repo.Head.Tip?.Tree, DiffTargets.Index, null, null, PatchOptions(repo))));
        });

    /// <summary>Local then remote-tracking branches, each group name-sorted, HEAD
    /// aliases dropped. Null when the folder is not a readable repository.</summary>
    public Task<IReadOnlyList<GitBranchRef>?> BranchesAsync(string folderPath)
        => RunAsync(folderPath, repo =>
        {
            var locals = repo.Branches
                .Where(b => !b.IsRemote && !b.FriendlyName.EndsWith("/HEAD", StringComparison.Ordinal))
                .Select(b => b.FriendlyName)
                .OrderBy(n => n, StringComparer.Ordinal)
                .Select(n => new GitBranchRef(n, GitBranchKind.Local));
            var remotes = repo.Branches
                .Where(b => b.IsRemote && !b.FriendlyName.EndsWith("/HEAD", StringComparison.Ordinal))
                .Select(b => b.FriendlyName)
                .OrderBy(n => n, StringComparer.Ordinal)
                .Select(n => new GitBranchRef(n, GitBranchKind.Remote));
            return (IReadOnlyList<GitBranchRef>)locals.Concat(remotes).ToList();
        });

    /// <summary>The publish target remote: "origin" when configured, else the first
    /// remote. Null when the repo has no remotes at all.</summary>
    public Task<string?> DefaultRemoteNameAsync(string folderPath)
        => RunAsync(folderPath, repo =>
            (repo.Network.Remotes["origin"] ?? repo.Network.Remotes.FirstOrDefault())?.Name);

    /// <summary>
    /// One commit's changed files with line counts (empty for a merge commit, whose
    /// combined diff prints nothing) plus the message body after the subject block.
    /// Null when the folder or the hash does not resolve.
    /// </summary>
    public Task<GitCommitDetails?> CommitDetailsAsync(string folderPath, string hash)
        => RunAsync(folderPath, repo =>
        {
            var commit = repo.Lookup<Commit>(hash);
            if (commit is null) return null;

            IReadOnlyList<GitChangedFile> files = Array.Empty<GitChangedFile>();
            if (commit.Parents.Count() <= 1)
            {
                var patch = repo.Diff.Compare<Patch>(commit.Parents.FirstOrDefault()?.Tree, commit.Tree, PatchOptions(repo));
                files = patch
                    .Select(entry => entry.LinesAdded == 0 && entry.LinesDeleted == 0
                        ? new GitChangedFile(entry.Path, string.Empty)
                        : new GitChangedFile(entry.Path, string.Empty, entry.LinesAdded, entry.LinesDeleted))
                    .ToList();
            }

            return new GitCommitDetails(hash, files, MessageBody(commit.Message));
        });

    /// <summary>One commit's patch restricted to a single file, capped for the fixed
    /// height patch box. Null when the folder or the hash does not resolve.</summary>
    public Task<string?> CommitFilePatchAsync(string folderPath, string hash, string path)
        => RunAsync(folderPath, repo =>
        {
            var commit = repo.Lookup<Commit>(hash);
            if (commit is null) return null;

            // Same combined-diff rule as the numstat: a merge renders nothing per file.
            if (commit.Parents.Count() > 1) return string.Empty;

            // Rename detection stays off for the single-file patch: a renamed path then
            // renders its full new content, which reads better in the drawer anyway.
            var patch = repo.Diff.Compare<Patch>(
                commit.Parents.FirstOrDefault()?.Tree,
                commit.Tree,
                new[] { path },
                null,
                PatchOptions(repo, detectRenames: false));
            return Cap(NormalizePatchPrefixes(patch.Content ?? string.Empty), CommitPatchReadCap, PatchTruncationSuffix);
        });

    /// <summary>
    /// Whether the commit is reachable from any fetched remote-tracking branch. No
    /// remote refs means not pushed; a folder or hash that does not resolve fails open
    /// (true), matching the CLI version's error behavior.
    /// </summary>
    public Task<bool> CommitPushedAsync(string folderPath, string hash)
        => Task.Run(() =>
        {
            try
            {
                using var repo = Open(folderPath);
                var commit = repo?.Lookup<Commit>(hash);
                if (commit is null) return true;

                var remoteTips = repo!.Branches
                    .Where(b => b.IsRemote && b.Tip is not null)
                    .Select(b => b.Tip!)
                    .Distinct()
                    .ToList();
                if (remoteTips.Count == 0) return false;

                foreach (var candidate in repo.Commits.QueryBy(new CommitFilter { IncludeReachableFrom = remoteTips }))
                {
                    if (candidate.Sha == commit.Sha) return true;
                }
                return false;
            }
            catch (DllNotFoundException ex)
            {
                LogNativeFailure(ex);
                return true;
            }
            catch (Exception ex)
            {
                Log.Logger.Debug(ex, "git pushed check failed for {FolderPath}", folderPath);
                return true;
            }
        });

    /// <summary>
    /// The batched pushed check: every hash reachable from a local branch but from no
    /// remote-tracking ref. Null when it cannot be answered honestly: no remote
    /// configured, or the folder is not a readable repository. An empty set means
    /// everything is pushed.
    /// </summary>
    public Task<IReadOnlySet<string>?> UnpushedHashesAsync(string folderPath)
        => RunAsync(folderPath, repo =>
        {
            if (!repo.Network.Remotes.Any()) return null;

            var localTips = repo.Branches
                .Where(b => !b.IsRemote && b.Tip is not null)
                .Select(b => b.Tip!)
                .ToList();
            var remoteTips = repo.Branches
                .Where(b => b.IsRemote && b.Tip is not null)
                .Select(b => b.Tip!)
                .ToList();

            var hashes = new HashSet<string>(StringComparer.Ordinal);
            if (localTips.Count == 0) return hashes;

            var unpushed = repo.Commits.QueryBy(new CommitFilter
            {
                IncludeReachableFrom = localTips,
                ExcludeReachableFrom = remoteTips,
            });
            foreach (var commit in unpushed)
            {
                hashes.Add(commit.Sha);
            }
            return (IReadOnlySet<string>)hashes;
        });

    /// <summary>The staged diff as a patch head for the commit-message prompt; empty
    /// when nothing is staged. Null when the folder is not a readable repository.</summary>
    public Task<string?> StagedPatchAsync(string folderPath)
        => RunAsync(folderPath, repo =>
        {
            var patch = repo.Diff.Compare<Patch>(repo.Head.Tip?.Tree, DiffTargets.Index);
            return Cap(patch.Content ?? string.Empty, StagedPatchReadCap, string.Empty);
        });

    /// <summary>The most recent commits, newest first (short of 40 chars the caller
    /// trims further). Null when the folder is not a readable repository.</summary>
    public Task<IReadOnlyList<GitCommitInfo>?> RecentCommitsAsync(string folderPath)
        => RunAsync(folderPath, repo =>
            (IReadOnlyList<GitCommitInfo>)repo.Commits
                .Take(10)
                .Select(c => new GitCommitInfo(c.Sha, c.MessageShort, c.Author.Name, c.Committer.When))
                .ToList());

    /// <summary>Upper bound on commits examined for one day's activity — the walk is
    /// newest-first but a rebased/old-dated commit could otherwise run it forever.</summary>
    private const int MaxDayScanCommits = 2000;

    /// <summary>Per-commit file entries fed to the daily-summary prompt.</summary>
    private const int MaxDayCommitFileEntries = 40;

    /// <summary>
    /// One local day's activity: the commits committer-dated inside
    /// <paramref name="day"/> (HEAD-reachable, newest first; merge commits carry no
    /// file stats, like the History drawer) plus the still-uncommitted working-tree
    /// state with merged staged+unstaged line counts — the two feeds the
    /// daily-summary prompt consumes. Null when the folder is not a readable
    /// repository.
    /// </summary>
    public Task<GitDayActivity?> DayActivityAsync(string folderPath, DateOnly day)
        => RunAsync(folderPath, repo =>
        {
            var midnight = day.ToDateTime(TimeOnly.MinValue);
            var commits = new List<GitDayCommit>();
            var examined = 0;
            foreach (var commit in repo.Commits)
            {
                if (++examined > MaxDayScanCommits) break;
                if (commit.Committer.When.LocalDateTime.Date != midnight) continue;

                IReadOnlyList<GitChangedFile> files = Array.Empty<GitChangedFile>();
                if (commit.Parents.Count() <= 1)
                {
                    var patch = repo.Diff.Compare<Patch>(commit.Parents.FirstOrDefault()?.Tree, commit.Tree, PatchOptions(repo));
                    files = patch
                        .Take(MaxDayCommitFileEntries)
                        .Select(entry => entry.LinesAdded == 0 && entry.LinesDeleted == 0
                            ? new GitChangedFile(entry.Path, string.Empty)
                            : new GitChangedFile(entry.Path, string.Empty, entry.LinesAdded, entry.LinesDeleted))
                        .ToList();
                }

                commits.Add(new GitDayCommit(commit.Sha, commit.MessageShort, commit.Author.Name, commit.Committer.When, files));
            }

            var status = repo.RetrieveStatus(StatusOptionsFor());
            var statusFiles = new List<GitChangedFile>();
            if (status.Any())
            {
                var staged = new List<GitChangedFile>();
                var unstaged = new List<GitChangedFile>();
                foreach (var entry in status)
                {
                    DescribeStatusEntry(entry, statusFiles, staged, unstaged);
                }

                var worktree = NumstatCounts(repo.Diff.Compare<Patch>(null, false, null, PatchOptions(repo)));
                var index = NumstatCounts(repo.Diff.Compare<Patch>(repo.Head.Tip?.Tree, DiffTargets.Index, null, null, PatchOptions(repo)));
                statusFiles = statusFiles
                    .Select(file => MergeDayCounts(file, worktree, index))
                    .ToList();
            }

            return new GitDayActivity(
                repo.Info.IsHeadDetached ? "(detached)" : repo.Head.FriendlyName,
                commits,
                statusFiles);
        });

    /// <summary>Fills one status row's counts from both diff sides (a path can be
    /// counted in the index diff, the worktree diff, or both); untracked stays
    /// count-less.</summary>
    private static GitChangedFile MergeDayCounts(
        GitChangedFile file,
        Dictionary<string, (int? Additions, int? Deletions)> worktree,
        Dictionary<string, (int? Additions, int? Deletions)> index)
    {
        if (file.StatusCode == "?") return file;
        worktree.TryGetValue(file.Path, out var wt);
        index.TryGetValue(file.Path, out var ix);
        int? additions = wt.Additions is null && ix.Additions is null ? null : (wt.Additions ?? 0) + (ix.Additions ?? 0);
        int? deletions = wt.Deletions is null && ix.Deletions is null ? null : (wt.Deletions ?? 0) + (ix.Deletions ?? 0);
        return additions is null && deletions is null ? file : file with { Additions = additions, Deletions = deletions };
    }

    /// <summary>The given paths the index knows (the discard flow's tracked/untracked
    /// split). Null when the folder is not a readable repository.</summary>
    public Task<HashSet<string>?> IndexedPathsAsync(string folderPath, IReadOnlyList<string> paths)
        => RunAsync(folderPath, repo =>
        {
            var index = repo.Index;
            var listed = new HashSet<string>(StringComparer.Ordinal);
            foreach (var path in paths)
            {
                if (index[path] is not null) listed.Add(path);
            }
            return listed;
        });

    /// <summary>Whether the index knows the path (the single-file discard's tracked vs
    /// untracked branch). False also on failure, like an empty ls-files output.</summary>
    public async Task<bool> IsTrackedAsync(string folderPath, string path)
    {
        try
        {
            return await Task.Run(() =>
            {
                using var repo = Open(folderPath);
                return repo is not null && repo.Index[path] is not null;
            });
        }
        catch (DllNotFoundException ex)
        {
            LogNativeFailure(ex);
            return false;
        }
        catch (Exception ex)
        {
            Log.Logger.Debug(ex, "git index check failed for {FolderPath}", folderPath);
            return false;
        }
    }

    private static StatusOptions StatusOptionsFor() => new()
    {
        Show = StatusShowOption.IndexAndWorkDir,
        IncludeUntracked = true,
        RecurseUntrackedDirs = true,
        IncludeIgnored = false,
        DetectRenamesInIndex = true,
        DetectRenamesInWorkDir = true,
    };

    /// <summary>
    /// Patch options matching what the git CLI would do: rename detection (git turns
    /// <c>diff.renames</c> on by itself since 2.9, but libgit2 only honors the config
    /// entry) and the repo's configured <c>diff.algorithm</c> (hunk anchoring differs
    /// between algorithms, and the user's global config applies to the CLI side).
    /// </summary>
    private static LibGit2Sharp.CompareOptions PatchOptions(Repository repo, bool detectRenames = true)
        => new()
        {
            Similarity = detectRenames ? SimilarityOptions.Renames : null,
            // git's CLI has the indent heuristic on by default; without it libgit2
            // anchors hunks one line off and the diff text subtly diverges.
            IndentHeuristic = true,
            Algorithm = repo.Config.Get<string>("diff.algorithm")?.Value?.Trim().ToLowerInvariant() switch
            {
                // libgit2's binding has no histogram; patience anchors hunks the same
                // way on ordinary diffs and is the closest of the three.
                "histogram" or "patience" => DiffAlgorithm.Patience,
                "minimal" => DiffAlgorithm.Minimal,
                _ => DiffAlgorithm.Myers,
            },
        };

    /// <summary>
    /// Normalizes the mnemonic diff prefixes a user's <c>diff.mnemonicPrefix</c> makes
    /// libgit2 emit (both commit sides as c/) back to the a/ b/ convention git show
    /// prints, so the drawer's patch text matches every other git tool. Anchored to
    /// the three header forms, so a path genuinely starting with c/ is never touched.
    /// </summary>
    private static string NormalizePatchPrefixes(string patch)
        => Regex.Replace(
            patch,
            @"^diff --git c/(.*) c/(.*)$|^--- c/|^\+\+\+ c/",
            m => m.Value.StartsWith("diff --git ", StringComparison.Ordinal)
                ? $"diff --git a/{m.Groups[1].Value} b/{m.Groups[2].Value}"
                : m.Value.StartsWith("--- ", StringComparison.Ordinal) ? "--- a/" : "+++ b/",
            RegexOptions.Multiline);

    /// <summary>
    /// Shapes one status entry into the porcelain-shaped rows the Changes tab's two
    /// views consume: the flat file list carries the verbatim XY pair (dots for the
    /// unmodified side), the split view one entry per non-dot side. Untracked entries
    /// are the "?" kind, worktree side only, exactly like porcelain's untracked lines.
    /// </summary>
    private static void DescribeStatusEntry(
        StatusEntry entry,
        List<GitChangedFile> statusFiles,
        List<GitChangedFile> staged,
        List<GitChangedFile> unstaged)
    {
        var path = entry.FilePath;
        if (string.IsNullOrEmpty(path)) return;
        var state = entry.State;

        if (state.HasFlag(FileStatus.Conflicted))
        {
            // Porcelain's unmerged XY pair (UD/DU/AA/…) is not exposed by libgit2's
            // status flags; both views carry the "U" the split view always used.
            unstaged.Add(new GitChangedFile(path, "U"));
            statusFiles.Add(new GitChangedFile(path, "U"));
            return;
        }

        if (state.HasFlag(FileStatus.NewInWorkdir))
        {
            unstaged.Add(new GitChangedFile(path, "?"));
            statusFiles.Add(new GitChangedFile(path, "?"));
            return;
        }

        var indexCode = IndexCode(state);
        var worktreeCode = WorktreeCode(state);
        if (indexCode is not '.')
        {
            staged.Add(new GitChangedFile(path, indexCode.ToString()));
        }
        if (worktreeCode is not '.')
        {
            unstaged.Add(new GitChangedFile(path, worktreeCode.ToString()));
        }
        statusFiles.Add(new GitChangedFile(path, $"{indexCode}{worktreeCode}"));
    }

    private static char IndexCode(FileStatus state)
    {
        if (state.HasFlag(FileStatus.NewInIndex)) return 'A';
        if (state.HasFlag(FileStatus.DeletedFromIndex)) return 'D';
        if (state.HasFlag(FileStatus.RenamedInIndex)) return 'R';
        if (state.HasFlag(FileStatus.TypeChangeInIndex)) return 'T';
        if (state.HasFlag(FileStatus.ModifiedInIndex)) return 'M';
        return '.';
    }

    private static char WorktreeCode(FileStatus state)
    {
        if (state.HasFlag(FileStatus.DeletedFromWorkdir)) return 'D';
        if (state.HasFlag(FileStatus.RenamedInWorkdir)) return 'R';
        if (state.HasFlag(FileStatus.TypeChangeInWorkdir)) return 'T';
        if (state.HasFlag(FileStatus.ModifiedInWorkdir)) return 'M';
        return '.';
    }

    private static Dictionary<string, (int? Additions, int? Deletions)> NumstatCounts(Patch patch)
    {
        var counts = new Dictionary<string, (int?, int?)>(StringComparer.Ordinal);
        foreach (var entry in patch)
        {
            // Binary content (numstat's "-") and mode-only changes both report zero
            // lines; the UI shows no counts for either, so they collapse to nulls.
            counts[entry.Path] = entry.LinesAdded == 0 && entry.LinesDeleted == 0
                ? (null, null)
                : (entry.LinesAdded, entry.LinesDeleted);
        }
        return counts;
    }

    /// <summary>The %b semantics: everything after the subject block, trimmed; a
    /// subject-only message has no body.</summary>
    private static string? MessageBody(string message)
    {
        var separator = message.IndexOf("\n\n", StringComparison.Ordinal);
        var body = separator < 0 ? string.Empty : message[(separator + 2)..];
        body = body.Trim();
        if (body.Length > CommitBodyReadCap)
        {
            body = body[..CommitBodyReadCap];
        }
        return body.Length == 0 ? null : body;
    }

    private static string Cap(string content, int cap, string suffix)
        => content.Length >= cap ? content[..cap] + suffix : content;

    private static Repository? Open(string folderPath)
    {
        var discovered = Repository.Discover(folderPath);
        return discovered is null ? null : new Repository(discovered);
    }

    private static Task<T?> RunAsync<T>(string folderPath, Func<Repository, T> action)
        => Task.Run(() =>
        {
            try
            {
                using var repo = Open(folderPath);
                if (repo is null) return default;
                return action(repo);
            }
            catch (DllNotFoundException ex)
            {
                LogNativeFailure(ex);
                return default;
            }
            catch (Exception ex)
            {
                Log.Logger.Debug(ex, "git read failed for {FolderPath}", folderPath);
                return default;
            }
        });

    private static void LogNativeFailure(Exception ex)
    {
        if (_nativeFailureLogged) return;
        _nativeFailureLogged = true;
        Log.Logger.Error(ex, "libgit2 native binaries failed to load; in-process git reads are disabled");
    }
}

/// <summary>
/// One repo's parsed change probes, shared by the Changes tab's two concurrent views:
/// the status entries already shaped for each (flat list, staged, unstaged) plus the
/// raw per-side numstat dictionaries, which the views apply with different merge
/// rules. Instances are read-only once returned; the projections build their own lists
/// on top.
/// </summary>
public sealed record GitChangeSnapshot(
    IReadOnlyList<GitChangedFile> StatusFiles,
    IReadOnlyList<GitChangedFile> StagedFiles,
    IReadOnlyList<GitChangedFile> UnstagedFiles,
    Dictionary<string, (int? Additions, int? Deletions)> WorktreeCounts,
    Dictionary<string, (int? Additions, int? Deletions)> CachedCounts)
{
    /// <summary>The shared snapshot for a clean repo (or an unreadable one).</summary>
    internal static readonly GitChangeSnapshot Empty = new(
        [],
        [],
        [],
        new Dictionary<string, (int?, int?)>(StringComparer.Ordinal),
        new Dictionary<string, (int?, int?)>(StringComparer.Ordinal));
}

/// <summary>
/// One commit made on the queried day, with the per-file line counts the
/// daily-summary prompt consumes (empty for a merge commit, whose combined diff
/// prints nothing).
/// </summary>
public sealed record GitDayCommit(
    string Hash,
    string Subject,
    string? Author,
    DateTimeOffset When,
    IReadOnlyList<GitChangedFile> Files);

/// <summary>
/// One repo's activity for a single local day: the commits committer-dated that day
/// plus the still-uncommitted working-tree state — "what happened" and "where things
/// left off" for the daily summary.
/// </summary>
public sealed record GitDayActivity(
    string? BranchName,
    IReadOnlyList<GitDayCommit> Commits,
    IReadOnlyList<GitChangedFile> UncommittedFiles)
{
    /// <summary>The shared empty activity (an unreadable repo, or nothing that day).</summary>
    public static GitDayActivity Empty { get; } = new(null, [], []);

    public bool IsEmpty => Commits.Count == 0 && UncommittedFiles.Count == 0;
}
