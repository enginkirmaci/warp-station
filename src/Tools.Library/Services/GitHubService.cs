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
/// All work is gated on <see cref="IRepoActivityService.IsEnabled"/> (the settings'
/// "Enable GitHub" flag plus a resolvable gh): a disabled service spawns no processes
/// at all, so hiding the column also stops the loading — the gating, refresh
/// coalescing and disabled-service guard come from <see cref="RepoActivityServiceBase{TActivity}"/>.
/// </para>
/// </summary>
public sealed class GitHubService : RepoActivityServiceBase<GitHubActivity>, IGitHubService
{
    /// <summary>Upper bound for a single gh invocation; a hung network call must not stall the pass.</summary>
    private static readonly TimeSpan ProcessTimeout = TimeSpan.FromSeconds(20);

    /// <summary>Caps each list fetch (and therefore the chip counts).</summary>
    private const int ItemLimit = 50;

    /// <summary>gh JSON field lists, kept minimal for cheap parsing.</summary>
    private const string PrFields = "number,title,url,author,labels,isDraft,headRefName,baseRefName,reviewDecision,updatedAt";
    private const string IssueFields = "number,title,url,author,labels,updatedAt";

    /// <summary>Set once <c>gh</c> is missing; subsequent refreshes become no-ops.</summary>
    private volatile bool _ghUnavailable;

    /// <summary>Absolute gh path resolved at Configure time; <c>null</c> when not resolvable.</summary>
    private volatile string? _ghPath;

    /// <summary>Static repo metadata per repo folder, backing the Overview sidebar's instant open.</summary>
    private readonly ConcurrentDictionary<string, GitHubRepoDetails?> _detailsByFolder = new(StringComparer.Ordinal);

    public GitHubService(IRepoService repoService)
        : base(repoService)
    {
    }

    /// <summary>A disabled GitHub service means the column flag is off, gh is not
    /// resolvable, or gh vanished between Configure and a spawn.</summary>
    protected override bool IsProviderReady => _ghPath is not null && !_ghUnavailable;

    protected override bool ResolveColumnFlag(ReposSettings settings) => settings.EnableGitHub;

    protected override string ServiceName => "GitHub";

    protected override void ConfigureProvider(ReposSettings settings)
    {
        // Resolve like every other configurable CLI: bare names walk PATH plus the
        // user-level bin directories a GUI session's PATH misses (memoized per name).
        _ghPath = ExecutableDefaults.Locate(settings.GitHubExecutable);
        if (ColumnEnabled && _ghPath is null)
        {
            Log.Logger.Warning(
                "GitHub column enabled but the gh CLI could not be located ({Configured}); set the GitHub CLI executable in Repos settings",
                settings.GitHubExecutable);
        }
    }

    /// <inheritdoc/>
    protected override Task FetchRepoAsync(Repo repo, CancellationToken cancellationToken)
        => RefreshRepoAsync(repo, cancellationToken);

    /// <inheritdoc/>
    public Task<GitHubActivity> RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default)
        => FetchGuardedAsync(repo, GitHubActivity.Empty, ct => RefreshRepoCoreAsync(repo, ct), cancellationToken);

    /// <summary>The gh queries themselves; only run while the service is enabled.</summary>
    private async Task<GitHubActivity> RefreshRepoCoreAsync(Repo repo, CancellationToken cancellationToken)
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
            StoreActivity(repo, activity);
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
    public GitHubActivity? GetCachedActivity(Repo repo) => CachedActivity(repo);

    /// <inheritdoc/>
    public async Task<GitHubRepoDetails?> GetRepoDetailsAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        var ghPath = _ghPath;
        if (ghPath is null || repo.FolderPath is null)
        {
            return null;
        }
        if (_detailsByFolder.TryGetValue(repo.FolderPath, out var cached))
        {
            return cached;
        }

        try
        {
            var json = await RunGhAsync(
                ghPath,
                repo.FolderPath,
                "repo view --json owner,createdAt,primaryLanguage,licenseInfo,repositoryTopics,defaultBranchRef,url",
                cancellationToken);
            var payload = string.IsNullOrWhiteSpace(json)
                ? null
                : JsonSerializer.Deserialize<RepoDetailsPayload>(json, JsonOptions);
            var details = payload is null
                ? null
                : new GitHubRepoDetails(
                    payload.Owner?.Login,
                    payload.CreatedAt,
                    payload.PrimaryLanguage?.Name,
                    payload.LicenseInfo?.Name,
                    payload.DefaultBranchRef?.Name,
                    payload.RepositoryTopics?.Where(t => !string.IsNullOrWhiteSpace(t.Name)).Select(t => t.Name!).ToArray() ?? [],
                    payload.Url);
            _detailsByFolder[repo.FolderPath] = details;
            return details;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "GitHub repo details failed for {FolderPath}", repo.FolderPath);
            return null;
        }
    }

    protected override void MarkUnavailable(Repo repo)
    {
        repo.GitHubRepoUrl = null;
        repo.GitHubPrCount = 0;
        repo.GitHubIssueCount = 0;
        repo.GitHubAvailable = false;
        repo.GitHubLoaded = true;
    }

    private static readonly JsonSerializerOptions JsonOptions = new() { PropertyNameCaseInsensitive = true };

    private sealed record RepoViewPayload(string? Url);

    private sealed record RepoDetailsPayload(
        OwnerPayload? Owner,
        DateTimeOffset? CreatedAt,
        NamePayload? PrimaryLanguage,
        NamePayload? LicenseInfo,
        NamePayload[]? RepositoryTopics,
        NamePayload? DefaultBranchRef,
        string? Url);

    private sealed record OwnerPayload(string? Login);

    private sealed record NamePayload(string? Name);

    private sealed record ItemPayload(
        int Number,
        string? Title,
        string? Url,
        AuthorPayload? Author,
        LabelPayload[]? Labels,
        bool IsDraft,
        string? HeadRefName = null,
        string? BaseRefName = null,
        string? ReviewDecision = null,
        DateTimeOffset? UpdatedAt = null);

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
                    isPullRequest && p.IsDraft,
                    isPullRequest ? p.HeadRefName : null,
                    isPullRequest ? p.BaseRefName : null,
                    isPullRequest ? p.ReviewDecision : null,
                    p.UpdatedAt))
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
