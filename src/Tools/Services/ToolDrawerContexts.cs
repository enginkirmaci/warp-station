using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Services;

/// <summary>
/// Context payload for the Add Repositories drawer component: the scan settings the
/// scan should honor, the currently tracked repos (shown locked as "Already added"),
/// and the completion source the component resolves with the checked repo paths when
/// the user confirms. <see cref="DialogService"/> awaits the completion; closing the
/// drawer any other way (✕, Cancel, backdrop, Escape, switching tools) cancels it.
/// </summary>
public sealed class AddRepositoriesDrawerContext
{
    public AddRepositoriesDrawerContext(ReposSettings settings, IReadOnlyList<Repo> trackedRepos)
    {
        Settings = settings;
        TrackedRepos = trackedRepos;
    }

    /// <summary>The repo scan settings (exclusions, patterns, depth) to scan with.</summary>
    public ReposSettings Settings { get; }

    /// <summary>The currently tracked repos; their findings cannot be re-added.</summary>
    public IReadOnlyList<Repo> TrackedRepos { get; }

    /// <summary>Resolved with the selected paths on confirm, or null on cancel.</summary>
    public TaskCompletionSource<IReadOnlyList<string>?> Completion { get; } =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
}

/// <summary>
/// Context payload for the Repo Settings drawer component: the settings instance to
/// edit and the completion source the component resolves with the edited settings when
/// the user saves (null on cancel — same semantics as the former modal dialog).
/// </summary>
public sealed class ReposSettingsDrawerContext
{
    public ReposSettingsDrawerContext(ReposSettings current)
    {
        Current = current;
    }

    /// <summary>The current repo settings to edit.</summary>
    public ReposSettings Current { get; }

    /// <summary>Resolved with the edited settings on save, or null on cancel.</summary>
    public TaskCompletionSource<ReposSettings?> Completion { get; } =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
}
