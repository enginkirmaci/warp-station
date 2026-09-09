using System.Net;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace Tools.Library.Services;

// Azure DevOps REST plumbing types shared by AzureDevOpsService: the per-call result
// wrapper and the payload records (matching the REST API responses; only the used
// fields are mapped).

/// <summary>Result of one REST call: the deserialized payload (when the status was a
/// success) and the status code itself, so callers can tell "no data" from "bad token".</summary>
internal sealed record RestResult<T>(T? Payload, HttpStatusCode Status)
    where T : class
{
    public bool IsSuccess => (int)Status is >= 200 and < 300;
}

internal static class AzureDevOpsRest
{
    public static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNameCaseInsensitive = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };
}

internal sealed record RepoMetaPayload(
    [property: JsonPropertyName("id")] string? Id,
    [property: JsonPropertyName("name")] string? Name,
    [property: JsonPropertyName("_links")] LinksPayload? Links);

internal sealed record LinksPayload([property: JsonPropertyName("web")] WebLinkPayload? Web);

internal sealed record WebLinkPayload([property: JsonPropertyName("href")] string? Href);

internal sealed record PrPayload(
    [property: JsonPropertyName("pullRequestId")] int PullRequestId,
    [property: JsonPropertyName("title")] string? Title,
    [property: JsonPropertyName("isDraft")] bool IsDraft,
    [property: JsonPropertyName("createdBy")] IdentityPayload? CreatedBy,
    [property: JsonPropertyName("labels")] LabelPayload[]? Labels,
    [property: JsonPropertyName("_links")] LinksPayload? Links);

internal sealed record IdentityPayload(
    [property: JsonPropertyName("displayName")] string? DisplayName,
    [property: JsonPropertyName("uniqueName")] string? UniqueName);

internal sealed record LabelPayload([property: JsonPropertyName("name")] string? Name);

internal sealed record PrListPayload([property: JsonPropertyName("value")] PrPayload[]? Value);

internal sealed record WiqlPayload(
    [property: JsonPropertyName("workItems")] WorkItemRefPayload[]? WorkItems);

internal sealed record WorkItemRefPayload([property: JsonPropertyName("id")] int Id);

internal sealed record WiqlRequest([property: JsonPropertyName("query")] string Query);

internal sealed record WorkItemPayload(
    [property: JsonPropertyName("id")] int Id,
    [property: JsonPropertyName("fields")] Dictionary<string, JsonElement>? Fields);

internal sealed record WorkItemListPayload([property: JsonPropertyName("value")] WorkItemPayload[]? Value);

internal sealed record BuildsPayload([property: JsonPropertyName("value")] BuildPayload[]? Value);

internal sealed record BuildPayload(
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

internal sealed record BuildRepoPayload([property: JsonPropertyName("id")] string? Id);

internal sealed record DefinitionPayload([property: JsonPropertyName("name")] string? Name);
