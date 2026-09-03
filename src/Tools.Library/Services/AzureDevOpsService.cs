using System.Collections.Concurrent;
using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Default <see cref="IAzureDevOpsService"/>. Unlike the GitHub column there is no
/// Azure DevOps CLI to lean on, so each repo's organization/project/repository is parsed
/// straight from the git remote (the <c>origin</c> URL in <c>.git/config</c> — HTTPS and
/// SSH forms both supported) and the Azure DevOps REST API is called with the
/// personal access token from the settings (env fallbacks: <c>AZURE_DEVOPS_PAT</c>,
/// <c>AZURE_DEVOPS_EXT_PAT</c>). Per repo: one metadata call proves the repo lives on
/// Azure DevOps and yields its id/web URL, then pull requests, open work items (the
/// hosting project's items — Azure DevOps does not scope work items to a repo) and
/// recent pipeline runs (client-filtered to this repo) are fetched in parallel.
/// Results are pushed onto the <see cref="Repo"/> entities from background threads,
/// exactly like <see cref="GitStatusService"/>.
/// <para>
/// All work is gated on <see cref="IsEnabled"/> (the settings' "Show Azure DevOps
/// column" flag plus a usable token): a disabled service sends no requests at all. A
/// refresh is additionally kicked automatically when <see cref="IRepoService"/> raises
/// <c>Changed</c> outside of a scan, mirroring the git status service. A token the
/// server rejects (401) short-circuits the rest of the pass instead of hammering the
/// API once per repo.
/// </para>
/// </summary>
public sealed class AzureDevOpsService : IAzureDevOpsService
{
    /// <summary>Upper bound for a single REST call; a hung request must not stall the pass.</summary>
    private static readonly TimeSpan RequestTimeout = TimeSpan.FromSeconds(20);

    /// <summary>How many repos are probed concurrently; keeps API traffic polite.</summary>
    private const int MaxParallelism = 3;

    /// <summary>Caps each pull-request / work-item list fetch (and therefore the chip counts).</summary>
    private const int ItemLimit = 50;

    /// <summary>How many recent project builds are fetched before client-filtering to the repo.</summary>
    private const int BuildFetchLimit = 30;

    /// <summary>How many of the repo's most recent pipeline runs the dialog keeps.</summary>
    private const int PipelineLimit = 10;

    /// <summary>WIQL: every not-done work item of the project, newest change first. The
    /// done-state names union the Agile, Scrum and Basic process templates; "Resolved"
    /// (Agile) intentionally still counts as open, like Azure DevOps' own queries.</summary>
    private const string WorkItemQuery =
        "SELECT [System.Id] FROM WorkItems " +
        "WHERE [System.TeamProject] = @project " +
        "AND [System.State] NOT IN ('Done', 'Closed', 'Removed', 'Completed') " +
        "ORDER BY [System.ChangedDate] DESC";

    private const string WorkItemFields = "System.Id,System.Title,System.State,System.WorkItemType,System.AssignedTo";

    private static readonly HttpClient Http = CreateHttpClient();

    private readonly IRepoService _repoService;

    /// <summary>Guards <see cref="_isRefreshing"/>/<see cref="_refreshPending"/>.</summary>
    private readonly object _sync = new();

    /// <summary>True while a refresh pass loop is running.</summary>
    private bool _isRefreshing;

    /// <summary>Set when a refresh is requested while one is running; runs another pass after.</summary>
    private bool _refreshPending;

    /// <summary>Volatile snapshot of the last Configure call (the VM reconfigures per navigation).</summary>
    private volatile bool _enabled;

    /// <summary>Personal access token from the last Configure; may be null (env fallback).</summary>
    private volatile string? _configuredPat;

    /// <summary>Set when the API rejects the token (401); short-circuits repos until the next Configure.</summary>
    private volatile bool _authRejected;

    /// <summary>Last fetched item lists per repo folder, backing the details dialog's instant open.</summary>
    private readonly ConcurrentDictionary<string, AzureDevOpsActivity> _activityByFolder = new(StringComparer.Ordinal);

    public AzureDevOpsService(IRepoService repoService)
    {
        _repoService = repoService;
        _repoService.Changed += OnRepoServiceChanged;
    }

    /// <inheritdoc/>
    public bool IsEnabled => _enabled && ResolveToken(_configuredPat) is not null && !_authRejected;

    /// <inheritdoc/>
    public void Configure(ReposSettings settings)
    {
        _enabled = settings.ShowAzureDevOpsColumn;
        _configuredPat = string.IsNullOrWhiteSpace(settings.AzureDevOpsPat) ? null : settings.AzureDevOpsPat.Trim();
        _authRejected = false;
        if (_enabled && ResolveToken(_configuredPat) is null)
        {
            Log.Logger.Warning(
                "Azure DevOps column enabled but no personal access token is configured; set one in Repos settings (or the AZURE_DEVOPS_PAT environment variable)");
        }
    }

    /// <summary>
    /// Re-checks Azure DevOps activity when fresh repo data arrives (cache load,
    /// completed rescan). Skipped while a scan is in flight — the completion
    /// notification follows right after — and while the column is disabled.
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
            Log.Logger.Debug(ex, "Azure DevOps activity refresh pass failed");
        }
    }

    /// <inheritdoc/>
    public async Task<AzureDevOpsActivity> RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default)
    {
        if (repo.FolderPath is null)
        {
            return AzureDevOpsActivity.Empty;
        }

        var token = ResolveToken(_configuredPat);
        if (token is null || _authRejected)
        {
            // No token (or a rejected one): settle the cell to its empty state so a
            // dialog opened from here shows the honest "no data" note instead of
            // spinning forever.
            return MarkUnavailable(repo);
        }

        // The remote decides everything: no Azure DevOps remote means the cell stays
        // empty for this repo — exactly the "not a GitHub repo" path of GitHubService.
        var remote = ParseAzureDevOpsRemote(repo.FolderPath);
        if (remote is null)
        {
            return MarkUnavailable(repo);
        }

        try
        {
            // First prove the repo lives on Azure DevOps and pick up its id (needed to
            // filter pipeline runs) and web URL. A 404 here means "not this project/repo";
            // a 401/403 means the token is bad (or too narrow) for the whole pass.
            var (repoId, repoWebUrl, status) = await GetRepoMetaAsync(remote, token, cancellationToken);
            if (repoId is null)
            {
                if (status is HttpStatusCode.Unauthorized or HttpStatusCode.Forbidden)
                {
                    _authRejected = true;
                    Log.Logger.Warning(
                        "Azure DevOps rejected the configured token (HTTP {Status}); Azure DevOps queries are paused until the next settings change",
                        (int)status);
                }
                return MarkUnavailable(repo);
            }

            // Then fetch the three activity kinds. They are independent, so run them together.
            var prTask = GetPullRequestsAsync(remote, repoId, token, cancellationToken);
            var workItemTask = GetWorkItemsAsync(remote, token, cancellationToken);
            var pipelineTask = GetPipelineRunsAsync(remote, repoId, token, cancellationToken);
            await Task.WhenAll(prTask, workItemTask, pipelineTask);

            var pullRequests = prTask.Result;
            var workItems = workItemTask.Result;
            var pipelineRuns = pipelineTask.Result;

            repo.AzureDevOpsRepoUrl = repoWebUrl;
            repo.AzureDevOpsPrCount = pullRequests.Count;
            repo.AzureDevOpsWorkItemCount = workItems.Count;
            SetPipelineSummary(repo, pipelineRuns);
            repo.AzureDevOpsAvailable = true;
            repo.AzureDevOpsLoaded = true;

            var activity = new AzureDevOpsActivity(pullRequests, workItems, pipelineRuns);
            if (repo.FolderPath is not null)
            {
                _activityByFolder[repo.FolderPath] = activity;
            }
            return activity;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Azure DevOps activity failed for {FolderPath}", repo.FolderPath);
            return MarkUnavailable(repo);
        }
    }

    /// <inheritdoc/>
    public AzureDevOpsActivity? GetCachedActivity(Repo repo)
        => repo.FolderPath is { } folder && _activityByFolder.TryGetValue(folder, out var activity)
            ? activity
            : null;

    private static AzureDevOpsActivity MarkUnavailable(Repo repo)
    {
        repo.AzureDevOpsRepoUrl = null;
        repo.AzureDevOpsPrCount = 0;
        repo.AzureDevOpsWorkItemCount = 0;
        repo.AzureDevOpsPipelineState = null;
        repo.AzureDevOpsPipelineInfo = null;
        repo.AzureDevOpsAvailable = false;
        repo.AzureDevOpsLoaded = true;
        return AzureDevOpsActivity.Empty;
    }

    /// <summary>
    /// Pushes the chip summary of the repo's most recent pipeline run: the state is the
    /// run's <c>result</c> once finished or its <c>status</c> while in flight, and the
    /// tooltip carries build number, definition and relative age.
    /// </summary>
    private static void SetPipelineSummary(Repo repo, IReadOnlyList<AzureDevOpsPipelineRun> runs)
    {
        var latest = runs.Count > 0 ? runs[0] : null;
        if (latest is null)
        {
            repo.AzureDevOpsPipelineState = null;
            repo.AzureDevOpsPipelineInfo = null;
            return;
        }

        repo.AzureDevOpsPipelineState = latest.IsRunning ? latest.Status : latest.Result;
        var stateText = latest.IsRunning ? "running" : latest.Result;
        var age = latest.FinishTime is { } at ? FormatRelative(at) : "just started";
        repo.AzureDevOpsPipelineInfo = $"Build #{latest.BuildNumber}" +
            (string.IsNullOrWhiteSpace(latest.DefinitionName) ? string.Empty : $" '{latest.DefinitionName}'") +
            $" — {stateText}, {age}";
    }

    /// <summary>Formats an age like the Repos table's relative labels ("5m ago").</summary>
    private static string FormatRelative(DateTimeOffset at)
    {
        var minutes = Math.Max(0, (int)(DateTimeOffset.Now - at).TotalMinutes);
        return minutes switch
        {
            < 1 => "just now",
            < 60 => $"{minutes}m ago",
            _ when minutes < 60 * 24 => $"{minutes / 60}h ago",
            _ when minutes < 60 * 24 * 7 => $"{minutes / (60 * 24)}d ago",
            _ when minutes < 60 * 24 * 30 => $"{minutes / (60 * 24 * 7)}w ago",
            _ when minutes < 60 * 24 * 365 => $"{minutes / (60 * 24 * 30)}mo ago",
            _ => $"{minutes / (60 * 24 * 365)}y ago",
        };
    }

    // --- Token ---

    /// <summary>
    /// Resolves the request token: the settings value first, then the environment
    /// variables conventionally used by Azure DevOps tooling. Returns
    /// <see langword="null"/> when none is set (the service then stays disabled).
    /// </summary>
    private static string? ResolveToken(string? configured)
    {
        if (!string.IsNullOrWhiteSpace(configured)) return configured.Trim();
        foreach (var name in new[] { "AZURE_DEVOPS_PAT", "AZURE_DEVOPS_EXT_PAT" })
        {
            var value = Environment.GetEnvironmentVariable(name);
            if (!string.IsNullOrWhiteSpace(value)) return value.Trim();
        }
        return null;
    }

    // --- Remote parsing ---

    /// <summary>
    /// The Azure DevOps coordinates parsed from a repo's git remote: the API base up to
    /// the organization (<c>https://dev.azure.com/{org}</c>) and the project and
    /// repository segments, already URL-escaped.
    /// </summary>
    internal sealed record AzureDevOpsRemote(string BaseUrl, string Project, string Repository);

    /// <summary>
    /// Parses the repo's <c>.git/config</c> for a remote URL hosted on Azure DevOps —
    /// no git process needed, and a GUI session's minimal PATH cannot break it.
    /// Recognizes <c>https://dev.azure.com/{org}/{project}/_git/{repo}</c>,
    /// the legacy <c>https://{org}.visualstudio.com/{project}/_git/{repo}</c> host and
    /// the <c>git@ssh.dev.azure.com:v3/{org}/{project}/{repo}</c> SSH form. The
    /// <c>origin</c> remote wins; otherwise the first remote with a URL is used.
    /// </summary>
    internal static AzureDevOpsRemote? ParseAzureDevOpsRemote(string folderPath)
    {
        var url = ReadRemoteUrl(folderPath, "origin") ?? ReadRemoteUrl(folderPath, null);
        return url is null ? null : ParseAzureDevOpsUrl(url);
    }

    /// <summary>
    /// Reads a remote's URL from <c>.git/config</c>: <paramref name="name"/> picks one
    /// remote; <see langword="null"/> returns the first remote section that has a URL.
    /// </summary>
    private static string? ReadRemoteUrl(string folderPath, string? name)
    {
        try
        {
            var configPath = Path.Combine(folderPath, ".git", "config");
            if (!File.Exists(configPath)) return null;

            var inSection = false;
            foreach (var rawLine in File.ReadLines(configPath))
            {
                var line = rawLine.Trim();
                if (line.StartsWith('['))
                {
                    inSection = name is not null
                        ? line.Equals($"[remote \"{name}\"]", StringComparison.OrdinalIgnoreCase)
                        : line.StartsWith("[remote", StringComparison.OrdinalIgnoreCase);
                    continue;
                }
                if (!inSection) continue;

                var eq = line.IndexOf('=');
                if (eq <= 0) continue;
                var key = line[..eq].Trim();
                if (!key.Equals("url", StringComparison.OrdinalIgnoreCase)) continue;

                var url = line[(eq + 1)..].Trim();
                if (url.Length > 0) return url;
            }
            return null;
        }
        catch (Exception ex)
        {
            Log.Logger.Debug(ex, "Failed reading git config for {FolderPath}", folderPath);
            return null;
        }
    }

    /// <summary>Parses an Azure DevOps remote URL; <see langword="null"/> for other hosts.</summary>
    internal static AzureDevOpsRemote? ParseAzureDevOpsUrl(string url)
    {
        // SSH: git@ssh.dev.azure.com:v3/{org}/{project}/{repo}
        var sshMarker = "ssh.dev.azure.com:v3/";
        var sshIndex = url.IndexOf(sshMarker, StringComparison.OrdinalIgnoreCase);
        if (sshIndex >= 0)
        {
            var parts = url[(sshIndex + sshMarker.Length)..].TrimEnd('/').Split('/');
            if (parts.Length < 3) return null;
            return new AzureDevOpsRemote(
                $"https://dev.azure.com/{Uri.EscapeDataString(parts[0])}",
                Uri.EscapeDataString(parts[1]),
                Uri.EscapeDataString(StripDotGit(parts[2])));
        }

        if (!url.StartsWith("http://", StringComparison.OrdinalIgnoreCase)
            && !url.StartsWith("https://", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }
        if (!Uri.TryCreate(url, UriKind.Absolute, out var uri)) return null;

        // AbsolutePath arrives escaped (%20); unescape first so the per-segment
        // EscapeDataString below does not double-encode it.
        var segments = Uri.UnescapeDataString(uri.AbsolutePath)
            .Split('/', StringSplitOptions.RemoveEmptyEntries);
        if (uri.Host.Equals("dev.azure.com", StringComparison.OrdinalIgnoreCase))
        {
            // HTTPS: https://dev.azure.com/{org}/{project}/_git/{repo}
            if (segments.Length >= 4 && segments[2].Equals("_git", StringComparison.OrdinalIgnoreCase))
            {
                return new AzureDevOpsRemote(
                    $"https://dev.azure.com/{Uri.EscapeDataString(segments[0])}",
                    Uri.EscapeDataString(segments[1]),
                    Uri.EscapeDataString(StripDotGit(segments[3])));
            }
            return null;
        }

        // Legacy HTTPS host: https://{org}.visualstudio.com/{project}/_git/{repo}
        if (uri.Host.EndsWith(".visualstudio.com", StringComparison.OrdinalIgnoreCase)
            && segments.Length >= 3
            && segments[1].Equals("_git", StringComparison.OrdinalIgnoreCase))
        {
            var org = uri.Host[..^".visualstudio.com".Length];
            return new AzureDevOpsRemote(
                $"https://dev.azure.com/{Uri.EscapeDataString(org)}",
                Uri.EscapeDataString(segments[0]),
                Uri.EscapeDataString(StripDotGit(segments[2])));
        }
        return null;
    }

    private static string StripDotGit(string segment)
        => segment.EndsWith(".git", StringComparison.OrdinalIgnoreCase) ? segment[..^4] : segment;

    // --- REST calls ---

    private static HttpClient CreateHttpClient()
    {
        var client = new HttpClient();
        client.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        client.Timeout = RequestTimeout;
        return client;
    }

    /// <summary>Result of one REST call: the deserialized payload (when the status was a
    /// success) and the status code itself, so callers can tell "no data" from "bad token".</summary>
    private sealed record RestResult<T>(T? Payload, HttpStatusCode Status)
        where T : class
    {
        public bool IsSuccess => (int)Status is >= 200 and < 300;
    }

    /// <summary>Sends an authenticated GET; never throws for HTTP errors.</summary>
    private static async Task<RestResult<T>> GetJsonAsync<T>(string url, string token, CancellationToken cancellationToken)
        where T : class
    {
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, url);
            ApplyAuth(request, token);
            using var timeoutCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            timeoutCts.CancelAfter(RequestTimeout);
            using var response = await Http.SendAsync(request, timeoutCts.Token);
            if (!response.IsSuccessStatusCode)
            {
                Log.Logger.Debug("Azure DevOps GET {Url} returned {Status}", url, (int)response.StatusCode);
                return new RestResult<T>(null, response.StatusCode);
            }
            var payload = await response.Content.ReadFromJsonAsync<T>(JsonOptions, timeoutCts.Token);
            return new RestResult<T>(payload, response.StatusCode);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Azure DevOps GET {Url} failed", url);
            return new RestResult<T>(null, HttpStatusCode.InternalServerError);
        }
    }

    /// <summary>Azure DevOps PATs authenticate as HTTP Basic with an empty username.</summary>
    private static void ApplyAuth(HttpRequestMessage request, string token)
        => request.Headers.Authorization = new AuthenticationHeaderValue(
            "Basic", Convert.ToBase64String(Encoding.UTF8.GetBytes($":{token}")));

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    // Payload records (matching the REST API responses; only the used fields are mapped).

    private sealed record RepoMetaPayload(
        [property: JsonPropertyName("id")] string? Id,
        [property: JsonPropertyName("name")] string? Name,
        [property: JsonPropertyName("_links")] LinksPayload? Links);

    private sealed record LinksPayload([property: JsonPropertyName("web")] WebLinkPayload? Web);

    private sealed record WebLinkPayload([property: JsonPropertyName("href")] string? Href);

    private sealed record PrPayload(
        [property: JsonPropertyName("pullRequestId")] int PullRequestId,
        [property: JsonPropertyName("title")] string? Title,
        [property: JsonPropertyName("isDraft")] bool IsDraft,
        [property: JsonPropertyName("createdBy")] IdentityPayload? CreatedBy,
        [property: JsonPropertyName("labels")] LabelPayload[]? Labels,
        [property: JsonPropertyName("_links")] LinksPayload? Links);

    private sealed record IdentityPayload(
        [property: JsonPropertyName("displayName")] string? DisplayName,
        [property: JsonPropertyName("uniqueName")] string? UniqueName);

    private sealed record LabelPayload([property: JsonPropertyName("name")] string? Name);

    private sealed record PrListPayload([property: JsonPropertyName("value")] PrPayload[]? Value);

    private sealed record WiqlPayload(
        [property: JsonPropertyName("workItems")] WorkItemRefPayload[]? WorkItems);

    private sealed record WorkItemRefPayload([property: JsonPropertyName("id")] int Id);

    private sealed record WiqlRequest([property: JsonPropertyName("query")] string Query);

    private sealed record WorkItemPayload(
        [property: JsonPropertyName("id")] int Id,
        [property: JsonPropertyName("fields")] Dictionary<string, JsonElement>? Fields);

    private sealed record WorkItemListPayload([property: JsonPropertyName("value")] WorkItemPayload[]? Value);

    private sealed record BuildsPayload([property: JsonPropertyName("value")] BuildPayload[]? Value);

    private sealed record BuildPayload(
        [property: JsonPropertyName("id")] int Id,
        [property: JsonPropertyName("buildNumber")] string? BuildNumber,
        [property: JsonPropertyName("status")] string? Status,
        [property: JsonPropertyName("result")] string? Result,
        [property: JsonPropertyName("sourceBranch")] string? SourceBranch,
        [property: JsonPropertyName("finishTime")] DateTimeOffset? FinishTime,
        [property: JsonPropertyName("repository")] BuildRepoPayload? Repository,
        [property: JsonPropertyName("definition")] DefinitionPayload? Definition,
        [property: JsonPropertyName("requestedFor")] IdentityPayload? RequestedFor,
        [property: JsonPropertyName("_links")] LinksPayload? Links);

    private sealed record BuildRepoPayload([property: JsonPropertyName("id")] string? Id);

    private sealed record DefinitionPayload([property: JsonPropertyName("name")] string? Name);

    /// <summary>
    /// Fetches the repo's id and web URL. Returns a null id when the repo is not on
    /// Azure DevOps (404) or the token was rejected — the status lets the caller tell
    /// the two apart.
    /// </summary>
    private static async Task<(string? Id, string? WebUrl, HttpStatusCode Status)> GetRepoMetaAsync(
        AzureDevOpsRemote remote, string token, CancellationToken cancellationToken)
    {
        var url = $"{remote.BaseUrl}/{remote.Project}/_apis/git/repositories/{remote.Repository}?api-version=7.1";
        var result = await GetJsonAsync<RepoMetaPayload>(url, token, cancellationToken);
        if (result.Payload?.Id is null)
        {
            return (null, null, result.Status);
        }
        return (result.Payload.Id, result.Payload.Links?.Web?.Href, result.Status);
    }

    /// <summary>Fetches the repo's active pull requests, oldest first.</summary>
    private static async Task<IReadOnlyList<AzureDevOpsItem>> GetPullRequestsAsync(
        AzureDevOpsRemote remote, string repoId, string token, CancellationToken cancellationToken)
    {
        var url = $"{remote.BaseUrl}/{remote.Project}/_apis/git/repositories/{repoId}/pullrequests" +
                  $"?api-version=7.1&searchCriteria.status=active&$top={ItemLimit}";
        var result = await GetJsonAsync<PrListPayload>(url, token, cancellationToken);
        if (result.Payload?.Value is not { } prs) return [];
        return prs
            .Where(p => p.PullRequestId > 0)
            .OrderBy(p => p.PullRequestId)
            .Select(p => new AzureDevOpsItem(
                p.PullRequestId,
                p.Title ?? string.Empty,
                p.Links?.Web?.Href ?? string.Empty,
                p.CreatedBy?.DisplayName ?? p.CreatedBy?.UniqueName,
                p.Labels?.Where(l => !string.IsNullOrWhiteSpace(l.Name)).Select(l => l.Name!).ToArray() ?? [],
                p.IsDraft,
                State: null))
            .ToArray();
    }

    /// <summary>
    /// Fetches the project's open work items: a WIQL query for the ids, then one batch
    /// call for the fields (WIQL results carry ids only). Ordered oldest-first like the
    /// GitHub dialog's lists.
    /// </summary>
    private static async Task<IReadOnlyList<AzureDevOpsItem>> GetWorkItemsAsync(
        AzureDevOpsRemote remote, string token, CancellationToken cancellationToken)
    {
        var ids = await QueryOpenWorkItemIdsAsync(remote, token, cancellationToken);
        if (ids.Length == 0) return [];

        var batchUrl = $"{remote.BaseUrl}/{remote.Project}/_apis/wit/workitems" +
                       $"?api-version=7.1&ids={string.Join(",", ids)}&fields={WorkItemFields}";
        var result = await GetJsonAsync<WorkItemListPayload>(batchUrl, token, cancellationToken);
        if (result.Payload?.Value is not { } items) return [];

        return items
            .Where(w => w.Id > 0 && GetFieldString(w.Fields, "System.Title") is not null)
            .OrderBy(w => w.Id)
            .Select(w => new AzureDevOpsItem(
                w.Id,
                GetFieldString(w.Fields, "System.Title")!,
                $"{remote.BaseUrl}/{remote.Project}/_workitems/edit/{w.Id}",
                GetFieldString(w.Fields, "System.AssignedTo"),
                GetFieldString(w.Fields, "System.WorkItemType") is { Length: > 0 } type ? [type] : [],
                IsDraft: false,
                State: GetFieldString(w.Fields, "System.State")))
            .ToArray();
    }

    /// <summary>Runs the WIQL query and returns up to <see cref="ItemLimit"/> open work item ids.</summary>
    private static async Task<int[]> QueryOpenWorkItemIdsAsync(
        AzureDevOpsRemote remote, string token, CancellationToken cancellationToken)
    {
        try
        {
            var wiqlUrl = $"{remote.BaseUrl}/{remote.Project}/_apis/wit/wiql?api-version=7.1&$top={ItemLimit}";
            using var request = new HttpRequestMessage(HttpMethod.Post, wiqlUrl);
            ApplyAuth(request, token);
            request.Content = JsonContent.Create(new WiqlRequest(WorkItemQuery));
            using var timeoutCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            timeoutCts.CancelAfter(RequestTimeout);
            using var response = await Http.SendAsync(request, timeoutCts.Token);
            if (!response.IsSuccessStatusCode)
            {
                Log.Logger.Debug("Azure DevOps WIQL for {Project} returned {Status}", remote.Project, (int)response.StatusCode);
                return [];
            }
            var wiql = await response.Content.ReadFromJsonAsync<WiqlPayload>(JsonOptions, timeoutCts.Token);
            return wiql?.WorkItems?.Select(w => w.Id).Where(id => id > 0).ToArray() ?? [];
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Azure DevOps WIQL query failed for {Project}", remote.Project);
            return [];
        }
    }

    /// <summary>Reads a flat string field from the work item's fields bag (identity
    /// fields arrive as objects and are read via their display name).</summary>
    private static string? GetFieldString(Dictionary<string, JsonElement>? fields, string key)
    {
        if (fields is null || !fields.TryGetValue(key, out var element)) return null;
        return element.ValueKind switch
        {
            JsonValueKind.String => element.GetString(),
            JsonValueKind.Object when element.TryGetProperty("displayName", out var name) => name.GetString(),
            _ => null,
        };
    }

    /// <summary>
    /// Fetches the project's most recent builds and keeps this repo's latest
    /// <see cref="PipelineLimit"/> runs, newest first. Builds are filtered client-side
    /// by repository id — the builds API has no server-side repository filter.
    /// </summary>
    private static async Task<IReadOnlyList<AzureDevOpsPipelineRun>> GetPipelineRunsAsync(
        AzureDevOpsRemote remote, string repoId, string token, CancellationToken cancellationToken)
    {
        var url = $"{remote.BaseUrl}/{remote.Project}/_apis/build/builds" +
                  $"?api-version=7.1&$top={BuildFetchLimit}&queryOrder=queueTimeDescending";
        var result = await GetJsonAsync<BuildsPayload>(url, token, cancellationToken);
        if (result.Payload?.Value is not { } builds) return [];

        return builds
            .Where(b => string.Equals(b.Repository?.Id, repoId, StringComparison.OrdinalIgnoreCase))
            .Take(PipelineLimit)
            .Select(b => new AzureDevOpsPipelineRun(
                b.Id,
                b.BuildNumber ?? b.Id.ToString(),
                b.Definition?.Name,
                b.SourceBranch,
                b.Result,
                b.Status,
                b.RequestedFor?.DisplayName ?? b.RequestedFor?.UniqueName,
                b.Links?.Web?.Href,
                b.FinishTime))
            .ToArray();
    }
}
