namespace Tools.Library.Entities;

/// <summary>
/// The lifecycle state of an opencode instance tracked by the serve integration.
/// Mirrored onto <see cref="Repo.OpenCodeStatus"/> so repo cards can bind it directly.
/// </summary>
public enum OpenCodeInstanceStatus
{
    /// <summary>
    /// No instance is running for the repo (the default). Either nothing has been launched,
    /// or the previously launched instance has exited.
    /// </summary>
    Stopped,

    /// <summary>
    /// An instance has been launched (terminal/process started) but the serve session has
    /// not yet been matched, so its live status is not yet known.
    /// </summary>
    Starting,

    /// <summary>
    /// A matching serve session exists and reports an idle/busy state — the instance is
    /// actively reachable through <c>opencode serve</c>.
    /// </summary>
    Running,

    /// <summary>
    /// The instance process exited unexpectedly or could not be matched to a serve session.
    /// </summary>
    Error,
}
