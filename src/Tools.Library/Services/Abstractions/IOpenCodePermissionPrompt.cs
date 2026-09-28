namespace Tools.Library.Services.Abstractions;

/// <summary>One tool-permission ask raised by an opencode serve run.</summary>
/// <param name="SessionId">The opencode session that raised the ask.</param>
/// <param name="RequestId">The ask's id — the reply routes echo it back.</param>
/// <param name="Kind">Raw opencode permission kind ("bash", "edit", "webfetch", …).</param>
/// <param name="Detail">Human-readable tool detail (the command, file path, url).</param>
public sealed record OpenCodePermissionRequest(string SessionId, string RequestId, string Kind, string? Detail);

/// <summary>
/// Surfaces opencode permission asks to the user. <c>true</c> = allow ("always" —
/// opencode stops asking for the same kind for the rest of the run), <c>false</c> =
/// reject (the tool call is denied and the run continues without it — a rejected
/// tool never fails the run). Must be callable from a background event pump and
/// must resolve as a reject through cancellation, so a dying run can never stall
/// on an ask nobody will answer.
/// </summary>
public interface IOpenCodePermissionPrompt
{
    Task<bool> AskAsync(OpenCodePermissionRequest request, CancellationToken cancellationToken);
}
