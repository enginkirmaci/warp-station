using System.Collections.Concurrent;
using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using AzureDevOpsRemote = Tools.Library.Services.AzureDevOpsRemoteParser.AzureDevOpsRemote;

namespace Tools.Library.Services;

/// <summary>
/// Default <see cref="IAzureDevOpsService"/>. Unlike the GitHub column there is no
/// Azure DevOps CLI to lean on, so each repo's organization/project/repository is parsed
/// straight from the git remote (the <c>origin</c> URL in <c>.git/config</c> — HTTPS and
/// SSH forms both supported, on the public hosts plus the settings' custom server URL
/// when one is configured — see <see cref="AzureDevOpsRemoteParser"/>) and the Azure
/// DevOps REST API is called with the personal access token from the settings (env
/// fallbacks: <c>AZURE_DEVOPS_PAT</c>, <c>AZURE_DEVOPS_EXT_PAT</c>). Per repo: one
/// metadata call proves the repo lives on Azure DevOps and yields its id/web URL, then
/// pull requests, open work items (the hosting project's items — Azure DevOps does not
/// scope work items to a repo) and recent pipeline runs (client-filtered to this repo)
/// are fetched in parallel. Results are pushed onto the <see cref="Repo"/> entities from
/// background threads, exactly like <see cref="GitStatusService"/>.
/// <para>
/// All work is gated on <see cref="IRepoActivityService.IsEnabled"/> (the settings'
/// "Enable Azure DevOps" flag plus a usable token): a disabled service sends no
/// requests at all. A refresh is additionally kicked automatically when
/// <see cref="IRepoService"/> raises <c>Changed</c> outside of a scan, mirroring the
/// git status service. A token the server rejects (401) short-circuits the rest of the
/// pass instead of hammering the API once per repo. The gating, refresh coalescing and
/// disabled-service guard come from <see cref="RepoActivityServiceBase{TActivity}"/>.
/// </para>
/// </summary>
public sealed class AzureDevOpsService : RepoActivityServiceBase<AzureDevOpsActivity>, IAzureDevOpsService
{
    /// <summary>Upper bound for a single REST call; a hung request must not stall the pass.</summary>
    private static readonly TimeSpan RequestTimeout = TimeSpan.FromSeconds(20);

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

    /// <summary>Personal access token from the last Configure; may be null (env fallback).</summary>
    private volatile string? _configuredPat;

    /// <summary>Normalized custom server URL (a company-hosted Azure DevOps Server base)
    /// from the last Configure; may be null (public hosts only).</summary>
    private volatile string? _configuredUrl;

    /// <summary>Set when the API rejects the token (401); short-circuits repos until the next Configure.</summary>
    private volatile bool _authRejected;

    /// <summary>
    /// Parsed Azure DevOps remotes per repo folder. A repo's <c>.git/config</c> only
    /// changes while the user edits remotes outside the app, so each folder is read and
    /// parsed once per session instead of on every refresh pass (the parse itself reads
    /// the config twice — once hunting <c>origin</c>, once for any remote). Cleared on
    /// every <see cref="ConfigureProvider"/> because the settings' custom server URL
    /// takes part in the parse: a newly configured host can make previously
    /// unrecognized remotes parse, and a removed one the reverse. Nulls are cached too
    /// (a non-Azure repo would otherwise re-read its config on every pass).
    /// </summary>
    private readonly ConcurrentDictionary<string, AzureDevOpsRemote?> _remotesByFolder = new(StringComparer.Ordinal);

    /// <summary>
    /// Work-item fetches keyed by project for the CURRENT refresh pass only (key
    /// <c>"{base}/{project}"</c>, which pins organization, collection and project):
    /// the work-item list is the hosting project's, so N repos of one project would
    /// otherwise run the identical WIQL + batch pair N times per pass. Task-valued so
    /// repos probing concurrently join one fetch instead of racing duplicates; nulled
    /// outside passes (see <see cref="RefreshPassAsync"/>) so out-of-pass single-repo
    /// refreshes always re-query and nothing can go stale between passes.
    /// </summary>
    private volatile ConcurrentDictionary<string, Lazy<Task<IReadOnlyList<AzureDevOpsItem>>>>? _passWorkItems;

    public AzureDevOpsService(IRepoService repoService)
        : base(repoService)
    {
    }

    /// <summary>A disabled Azure DevOps service means the column flag is off, no token
    /// is resolvable, or the server rejected the token.</summary>
    protected override bool IsProviderReady => ResolveToken(_configuredPat) is not null && !_authRejected;

    protected override bool ResolveColumnFlag(ReposSettings settings) => settings.EnableAzureDevOps;

    protected override string ServiceName => "Azure DevOps";

    protected override void ConfigureProvider(ReposSettings settings)
    {
        _configuredPat = string.IsNullOrWhiteSpace(settings.AzureDevOpsPat) ? null : settings.AzureDevOpsPat.Trim();
        _configuredUrl = AzureDevOpsRemoteParser.NormalizeServerUrl(settings.AzureDevOpsUrl);
        _authRejected = false;
        // The custom server URL takes part in remote parsing, so memoized remotes from
        // the previous settings are stale the moment it changes.
        _remotesByFolder.Clear();
        if (ColumnEnabled && ResolveToken(_configuredPat) is null)
        {
            Log.Logger.Warning(
                "Azure DevOps column enabled but no personal access token is configured; set one in Repos settings (or the AZURE_DEVOPS_PAT environment variable)");
        }
    }

    /// <inheritdoc/>
    protected override Task FetchRepoAsync(Repo repo, CancellationToken cancellationToken)
        => RefreshRepoAsync(repo, cancellationToken);

    /// <inheritdoc/>
    public Task<AzureDevOpsActivity> RefreshRepoAsync(Repo repo, CancellationToken cancellationToken = default)
        => FetchGuardedAsync(repo, AzureDevOpsActivity.Empty, ct => RefreshRepoCoreAsync(repo, ct), cancellationToken);

    /// <summary>
    /// One throttled refresh pass, and the scope of the per-project work-item
    /// memoization: the cache exists exactly for the duration of a pass (the coalescer
    /// never stacks concurrent passes), so the next pass always re-queries and repos
    /// refreshed outside a pass bypass it entirely.
    /// </summary>
    protected override async Task RefreshPassAsync(CancellationToken cancellationToken)
    {
        _passWorkItems = new ConcurrentDictionary<string, Lazy<Task<IReadOnlyList<AzureDevOpsItem>>>>();
        try
        {
            await base.RefreshPassAsync(cancellationToken);
        }
        finally
        {
            _passWorkItems = null;
        }
    }

    /// <summary>The REST queries themselves; only run while the service is enabled.</summary>
    private async Task<AzureDevOpsActivity> RefreshRepoCoreAsync(Repo repo, CancellationToken cancellationToken)
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
            MarkUnavailable(repo);
            return AzureDevOpsActivity.Empty;
        }

        // The remote decides everything: no Azure DevOps remote means the cell stays
        // empty for this repo — exactly the "not a GitHub repo" path of GitHubService.
        var remote = ParseAzureDevOpsRemote(repo.FolderPath);
        if (remote is null)
        {
            MarkUnavailable(repo);
            return AzureDevOpsActivity.Empty;
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
                MarkUnavailable(repo);
                return AzureDevOpsActivity.Empty;
            }

            // Then fetch the three activity kinds. They are independent, so run them together.
            var prTask = GetPullRequestsAsync(remote, repoId, token, cancellationToken);
            var workItemTask = GetWorkItemsForProjectAsync(remote, token, cancellationToken);
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
            StoreActivity(repo, activity);
            return activity;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Azure DevOps activity failed for {FolderPath}", repo.FolderPath);
            MarkUnavailable(repo);
            return AzureDevOpsActivity.Empty;
        }
    }

    /// <inheritdoc/>
    public AzureDevOpsActivity? GetCachedActivity(Repo repo) => CachedActivity(repo);

    protected override void MarkUnavailable(Repo repo)
    {
        repo.AzureDevOpsRepoUrl = null;
        repo.AzureDevOpsPrCount = 0;
        repo.AzureDevOpsWorkItemCount = 0;
        repo.AzureDevOpsPipelineState = null;
        repo.AzureDevOpsPipelineInfo = null;
        repo.AzureDevOpsAvailable = false;
        repo.AzureDevOpsLoaded = true;
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
        var age = latest.FinishTime is { } at ? Formatters.RelativeTime.Format(at) : "just started";
        repo.AzureDevOpsPipelineInfo = $"Build #{latest.BuildNumber}" +
            (string.IsNullOrWhiteSpace(latest.DefinitionName) ? string.Empty : $" '{latest.DefinitionName}'") +
            $" — {stateText}, {age}";
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

    // --- Remote parsing (the mechanics live in AzureDevOpsRemoteParser; this owns the
    //     per-folder memoization keyed to the configured settings) ---

    /// <summary>
    /// Parses the repo's <c>.git/config</c> for a remote URL hosted on Azure DevOps,
    /// memoized per folder (<see cref="_remotesByFolder"/>) so a refresh pass reads each
    /// repo's config once per session instead of twice per pass.
    /// </summary>
    private AzureDevOpsRemote? ParseAzureDevOpsRemote(string folderPath)
        => _remotesByFolder.GetOrAdd(folderPath, ParseRemoteCore);

    /// <summary>The uncached parse behind <see cref="ParseAzureDevOpsRemote"/>.</summary>
    private AzureDevOpsRemote? ParseRemoteCore(string folderPath)
    {
        var url = AzureDevOpsRemoteParser.ReadRemoteUrl(folderPath, "origin")
                  ?? AzureDevOpsRemoteParser.ReadRemoteUrl(folderPath, null);
        return url is null ? null : AzureDevOpsRemoteParser.ParseUrl(url, _configuredUrl);
    }

    // --- REST calls ---

    private static HttpClient CreateHttpClient()
    {
        var client = new HttpClient();
        client.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        client.Timeout = RequestTimeout;
        return client;
    }

    /// <summary>
    /// Sends an authenticated request and reads the JSON payload; never throws for HTTP
    /// errors. GETs carry no body, the WIQL POST sends its query as JSON — both share
    /// the auth header, the per-call timeout and the error logging here.
    /// </summary>
    private static async Task<RestResult<T>> SendJsonAsync<T>(
        HttpMethod method, string url, string token, object? content, CancellationToken cancellationToken)
        where T : class
    {
        try
        {
            using var request = new HttpRequestMessage(method, url);
            ApplyAuth(request, token);
            if (content is not null)
            {
                request.Content = JsonContent.Create(content, options: AzureDevOpsRest.JsonOptions);
            }
            using var timeoutCts = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            timeoutCts.CancelAfter(RequestTimeout);
            using var response = await Http.SendAsync(request, timeoutCts.Token);
            if (!response.IsSuccessStatusCode)
            {
                Log.Logger.Debug("Azure DevOps {Method} {Url} returned {Status}", method.Method, url, (int)response.StatusCode);
                return new RestResult<T>(null, response.StatusCode);
            }
            var payload = await response.Content.ReadFromJsonAsync<T>(AzureDevOpsRest.JsonOptions, timeoutCts.Token);
            return new RestResult<T>(payload, response.StatusCode);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Log.Logger.Debug(ex, "Azure DevOps {Method} {Url} failed", method.Method, url);
            return new RestResult<T>(null, HttpStatusCode.InternalServerError);
        }
    }

    /// <summary>Azure DevOps PATs authenticate as HTTP Basic with an empty username.</summary>
    private static void ApplyAuth(HttpRequestMessage request, string token)
        => request.Headers.Authorization = new AuthenticationHeaderValue(
            "Basic", Convert.ToBase64String(Encoding.UTF8.GetBytes($":{token}")));

    /// <summary>
    /// Fetches the repo's id and web URL. Returns a null id when the repo is not on
    /// Azure DevOps (404) or the token was rejected — the status lets the caller tell
    /// the two apart.
    /// </summary>
    private static async Task<(string? Id, string? WebUrl, HttpStatusCode Status)> GetRepoMetaAsync(
        AzureDevOpsRemote remote, string token, CancellationToken cancellationToken)
    {
        var url = $"{remote.BaseUrl}/{remote.Project}/_apis/git/repositories/{remote.Repository}?api-version=7.1";
        var result = await SendJsonAsync<RepoMetaPayload>(HttpMethod.Get, url, token, null, cancellationToken);
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
        var result = await SendJsonAsync<PrListPayload>(HttpMethod.Get, url, token, null, cancellationToken);
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
    /// <see cref="GetWorkItemsAsync"/> behind the per-pass, per-project memoization:
    /// every repo of one project shares a single WIQL + batch fetch per pass. The
    /// <see cref="Lazy{T}"/> wrapper keeps concurrent repos from racing duplicate HTTP
    /// calls — the losing factory's task never starts. Outside a pass (no cache, see
    /// <see cref="RefreshPassAsync"/>) the fetch runs directly. The whole pass shares
    /// one token, so the first repo's captured arguments are every repo's.
    /// </summary>
    private Task<IReadOnlyList<AzureDevOpsItem>> GetWorkItemsForProjectAsync(
        AzureDevOpsRemote remote, string token, CancellationToken cancellationToken)
    {
        var cache = _passWorkItems;
        if (cache is null) return GetWorkItemsAsync(remote, token, cancellationToken);

        return cache.GetOrAdd(
            $"{remote.BaseUrl}/{remote.Project}",
            _ => new Lazy<Task<IReadOnlyList<AzureDevOpsItem>>>(
                () => GetWorkItemsAsync(remote, token, cancellationToken))).Value;
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
        var result = await SendJsonAsync<WorkItemListPayload>(HttpMethod.Get, batchUrl, token, null, cancellationToken);
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
        var wiqlUrl = $"{remote.BaseUrl}/{remote.Project}/_apis/wit/wiql?api-version=7.1&$top={ItemLimit}";
        var result = await SendJsonAsync<WiqlPayload>(
            HttpMethod.Post, wiqlUrl, token, new WiqlRequest(WorkItemQuery), cancellationToken);
        return result.Payload?.WorkItems?.Select(w => w.Id).Where(id => id > 0).ToArray() ?? [];
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
        var result = await SendJsonAsync<BuildsPayload>(HttpMethod.Get, url, token, null, cancellationToken);
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
