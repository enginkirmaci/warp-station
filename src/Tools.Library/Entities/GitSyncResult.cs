namespace Tools.Library.Entities;

/// <summary>
/// Outcome of one repo sync command (clone, pull or push): whether the git invocation
/// succeeded and, on failure, the actionable stderr line ("no upstream configured",
/// "rejected (fetch first)", …) for the failure notification. A cancelled run reports
/// neither success nor a git error — the user aborted it.
/// </summary>
public sealed record GitSyncResult(bool Success, string? Error, bool Cancelled = false)
{
    public static GitSyncResult Ok() => new(true, null);
}
