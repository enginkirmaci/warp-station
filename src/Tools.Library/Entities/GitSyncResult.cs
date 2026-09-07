namespace Tools.Library.Entities;

/// <summary>
/// Outcome of one repo sync command (pull or push): whether the git invocation
/// succeeded and, on failure, the actionable stderr line ("no upstream configured",
/// "rejected (fetch first)", …) for the failure notification.
/// </summary>
public sealed record GitSyncResult(bool Success, string? Error)
{
    public static GitSyncResult Ok() => new(true, null);
}
