using System.Collections.Concurrent;
using System.ComponentModel;
using System.Diagnostics;
using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Default <see cref="IGitHubService"/>. Queries the <c>gh</c> CLI per repo (running
/// inside the repo folder so gh resolves owner/name from the git remote): one
/// <c>gh repo view --json url</c> proves the repo lives on GitHub and yields the
/// column's repo link, then <c>gh pr list</c> and <c>gh issue list</c> fetch the open
/// items whose counts feed the column chips and whose full lists back the details
/// dialog. Results are pushed onto the <see cref="Repo"/> entities from background
/// threads, exactly like <see cref="GitStatusService"/>.
/// <para>
/// All work is gated on <see cref="IsEnabled"/> (the settings' "Show GitHub column"
/// flag): a disabled service spawns no processes at all, so hiding the column also
/// stops the loading. A refresh is additionally kicked automatically when
/// <see cref="IRepoService"/> raises <c>Changed</c> outside of a scan, mirroring the
/// git status service.
/// </para>
/// </summary>
public sealed class GitHubService : IGitHubService
{
    /// <summary>Upper bound for a single gh invocation; a hung network call must not stall the pass.</summary>
    private static readonly TimeSpan ProcessTimeout = TimeSpan.FromSeconds(20);

    /// <summary>How many repos are probed concurrently; keeps API traffic polite.</summary>
    private const int MaxParallelism = 3;

    /// <summary>Caps each list fetch (and therefore the chip counts).</summary>
    private const int ItemLimit = 50;

    /// <summary>gh JSON field lists, kept minimal for cheap parsing.</summary>
    private const string PrFields = "number,title,url,author,labels,isDraft";
    private const string IssueFields = "number,title,url,author,labels";

    private readonly IRepoService _repoService;

    /// <summary>Guards <see cref="_isRefreshing"/>/<see cref="_refreshPending"/>.</summary>
    private readonly object _sync = new();

    /// <summary>True while a refresh pass loop is running.</summary>
    private bool _isRefreshing;

    /// <summary>Set when a refresh is requested while one is running; runs another pass after.</summary>
    private bool _refreshPending;

    /// <summary>Set once <c>gh</c> is missing; subsequent refreshes become no-ops.</summary>
    private volatile bool _ghUnavailable;

    /// <summary>Volatile snapshot of the last Configure call (the VM reconfigures per navigation).</summary>
    private volatile bool _enabled;

    /// <summary>Absolute gh path resolved at Configure time; <c>null</c> when not resolvable.</summary>
    private volatile string? _ghPath;

    /// <summary>Last fetched item lists per repo folder, backing the details dialog's instant open.</summary>
    private readonly ConcurrentDictionary<string, GitHubActivity> _activityByFolder = new(StringComparer.Ordinal);

    public GitHubService(IRepoService repoService)
    {
        _repoService = repoService;
        _repoService.Changed += OnRepoServiceChanged;
    }

    /// <inheritdoc/>
    public bool IsEnabled => _enabled && _ghPath is not null && !_ghUnavailable;

    /// <inheritdoc/>
    public void Configure(ReposSettings settings)
    {
        _enabled = settings.ShowGitHubColumn;
        // Resolve like every other configurable CLI: bare names walk PATH plus the
        // user-level bin directories a GUI session's PATH misses (memoized per name).
        _ghPath = ExecutableDefaults.Locate(settings.GitHubExecutable);
        if (_enabled && _ghPath is null)
        {
            Log.Logger.Warning(
                "GitHub column enabled but the gh CLI could not be located ({Configured}); set the GitHub CLI executable in Repos settings",
                settings.GitHubExecutable);
        }
    }

    /// <summary>
    /// Re-checks GitHub activity when fresh repo data arrives (cache load, completed
    /// rescan). Skipped while a scan is in flight — the completion notification follows
    /// right after — and while the column is disabled.
    /// </summary>
    private void OnRepoServiceChanged(object? sender, EventArgs e)
    {
        if (!_enabled || _repoService.IsBusy || _repoService.Repos.Count == 0) return;
        _ = RefreshAllAsync();
    }

    /// <inheritdoc/>
    public async Task RefreshAllAsync(CancellationToken cancellationToken = default)
    {
        if (!IsEnabled) return;

        // Coalesce concurrent triggers exactly like GitStatusService: while a pass runs,
        // callers just flag a follow-up pass, and the loop drains pending flags.
        lock (_sync)
        {
            if (_isRefreshing)
            {
                _refreshPending = true;
                return;
            }
            _isRefreshing = true;
        }

        try
        {
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
                        return;
                    }
                }
            }
        }
        finally
        {
            lock (_sync)
            {
                _isRefreshing = false;
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
            Log.Logger.Debug(ex, "GitHub activity refresh pass failed");
        }
    }

    /// <inheritdoc/>
    public async Task<GitHubActivity> RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        var ghPath = _ghPath;
        if (ghPath is null || repo.FolderPath is null)
        {
            return GitHubActivity.Empty;
        }

        try
        {
            // First prove the repo lives on GitHub and pick up its HTML URL in one call.
            // A failure here means "not a GitHub repo" (or gh/auth trouble) — mark the
            // repo unavailable so its cell stays empty instead of showing a misleading OK.
            var repoJson = await RunGhAsync(ghPath, repo.FolderPath, "repo view --json url", cancellationToken);
            var repoUrl = string.IsNullOrWhiteSpace(repoJson)
                ? null
                : JsonSerializer.Deserialize<RepoViewPayload>(repoJson, JsonOptions)?.Url;
            if (string.IsNullOrWhiteSpace(repoUrl))
            {
                MarkUnavailable(repo);
                return GitHubActivity.Empty;
            }

            // Then fetch both open-item lists. They are independent, so run them together;
            // each is one gh process in the repo folder.
            var prTask = RunGhAsync(ghPath, repo.FolderPath, $"pr list --json {PrFields} --limit {ItemLimit}", cancellationToken);
            var issueTask = RunGhAsync(ghPath, repo.FolderPath, $"issue list --json {IssueFields} --limit {ItemLimit}", cancellationToken);
            await Task.WhenAll(prTask, issueTask);

            var pullRequests = ParseItems(prTask.Result, isPullRequest: true);
            var issues = ParseItems(issueTask.Result, isPullRequest: false);

            repo.GitHubRepoUrl = repoUrl;
            repo.GitHubPrCount = pullRequests.Count;
            repo.GitHubIssueCount = issues.Count;
            repo.GitHubAvailable = true;
            repo.GitHubLoaded = true;

            var activity = new GitHubActivity(pullRequests, issues);
            if (repo.FolderPath is not null)
            {
                _activityByFolder[repo.FolderPath] = activity;
            }
            return activity;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "GitHub activity failed for {FolderPath}", repo.FolderPath);
            MarkUnavailable(repo);
            return GitHubActivity.Empty;
        }
    }

    /// <inheritdoc/>
    public GitHubActivity? GetCachedActivity(Repo repo)
        => repo.FolderPath is { } folder && _activityByFolder.TryGetValue(folder, out var activity)
            ? activity
            : null;

    private static void MarkUnavailable(Repo repo)
    {
        repo.GitHubRepoUrl = null;
        repo.GitHubPrCount = 0;
        repo.GitHubIssueCount = 0;
        repo.GitHubAvailable = false;
        repo.GitHubLoaded = true;
    }

    private static readonly JsonSerializerOptions JsonOptions = new() { PropertyNameCaseInsensitive = true };

    private sealed record RepoViewPayload(string? Url);

    private sealed record ItemPayload(int Number, string? Title, string? Url, AuthorPayload? Author, LabelPayload[]? Labels, bool IsDraft);

    private sealed record AuthorPayload(string? Login);

    private sealed record LabelPayload(string? Name);

    /// <summary>
    /// Parses a <c>gh list</c> JSON array into dialog items, ordered by number ascending
    /// so the dialog reads oldest-first like GitHub's own list pages.
    /// </summary>
    private static IReadOnlyList<GitHubItem> ParseItems(string? json, bool isPullRequest)
    {
        if (string.IsNullOrWhiteSpace(json))
        {
            return [];
        }

        try
        {
            var payload = JsonSerializer.Deserialize<ItemPayload[]>(json, JsonOptions);
            if (payload is null)
            {
                return [];
            }

            return payload
                .Where(p => p.Number > 0 && !string.IsNullOrWhiteSpace(p.Url))
                .OrderBy(p => p.Number)
                .Select(p => new GitHubItem(
                    p.Number,
                    p.Title ?? string.Empty,
                    p.Url!,
                    p.Author?.Login,
                    p.Labels?.Where(l => !string.IsNullOrWhiteSpace(l.Name)).Select(l => l.Name!).ToArray() ?? [],
                    isPullRequest && p.IsDraft))
                .ToArray();
        }
        catch (JsonException ex)
        {
            Log.Logger.Debug(ex, "Failed to parse gh list output");
            return [];
        }
    }

    /// <summary>
    /// Runs <c>gh</c> with the given arguments inside <paramref name="workingDir"/> and
    /// returns stdout, or <see langword="null"/> on any failure (non-zero exit, timeout,
    /// missing binary). Terminal prompts are disabled so probing never blocks.
    /// </summary>
    private async Task<string?> RunGhAsync(string ghPath, string workingDir, string arguments, CancellationToken cancellationToken)
    {
        var startInfo = new ProcessStartInfo
        {
            FileName = ghPath,
            Arguments = arguments,
            WorkingDirectory = workingDir,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        // Never block on credential prompts — gh falls back to its keyring/token config
        // and fails fast instead when it cannot authenticate.
        startInfo.Environment["GIT_TERMINAL_PROMPT"] = "0";
        startInfo.Environment["GH_PROMPT"] = "disabled";

        using var process = new Process { StartInfo = startInfo };
        try
        {
            if (!process.Start())
                return null;
        }
        catch (Win32Exception ex)
        {
            // gh vanished between Configure and spawn: disable until the next app run
            // instead of failing every repo on every refresh.
            _ghUnavailable = true;
            Log.Logger.Debug(ex, "gh executable not found; GitHub queries disabled");
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
            // Timeout, not an external cancel: kill the stray gh process and move on.
            try { process.Kill(entireProcessTree: true); } catch { /* already exited */ }
            return null;
        }
    }
}
