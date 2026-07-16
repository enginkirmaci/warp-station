namespace Tools.Library.Services.OpenCode;

/// <summary>
/// The flattened view of <c>GET /config/providers</c> consumed by the OpenCode model
/// selector: the <c>provider/model-id</c> ids in display order and the default model id
/// (if any is configured). <see cref="Empty"/> is the not-connected / failure sentinel.
/// </summary>
public sealed record ServeModelsResult(IReadOnlyList<string> Models, string? DefaultModel)
{
    /// <summary>
    /// Returned by <see cref="IOpenCodeServeService.GetModelsAsync"/> when serve is not
    /// connected or the providers list is empty/unreadable, so the selector can show its
    /// "connect serve" hint.
    /// </summary>
    public static ServeModelsResult Empty { get; } = new(Array.Empty<string>(), null);
}
