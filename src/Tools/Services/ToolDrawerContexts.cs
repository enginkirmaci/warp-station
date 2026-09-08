using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

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

    /// <summary>The repo scan settings (exclusions, patterns) to scan with.</summary>
    public ReposSettings Settings { get; }

    /// <summary>The currently tracked repos; their findings cannot be re-added.</summary>
    public IReadOnlyList<Repo> TrackedRepos { get; }

    /// <summary>Resolved with the selected paths on confirm, or null on cancel.</summary>
    public TaskCompletionSource<IReadOnlyList<string>?> Completion { get; } =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
}

/// <summary>
/// Context payload for the Repo Settings drawer component: the settings instances to
/// edit (Repos plus the OpenCode models and the NuGet enable flag, which live in
/// settings here since those settings moved out of their tool surfaces) and the
/// completion source the component resolves with the edited sections when the user
/// saves (null on cancel — same semantics as the former modal dialog).
/// </summary>
public sealed class ReposSettingsDrawerContext
{
    public ReposSettingsDrawerContext(ReposSettings current, OpenCodeSettings openCode, bool nugetEnabled)
    {
        Current = current;
        OpenCode = openCode;
        NugetEnabled = nugetEnabled;
    }

    /// <summary>The current repo settings to edit.</summary>
    public ReposSettings Current { get; }

    /// <summary>The current OpenCode settings (default/commit models) to edit.</summary>
    public OpenCodeSettings OpenCode { get; }

    /// <summary>The current NuGet enable flag to edit.</summary>
    public bool NugetEnabled { get; }

    /// <summary>Resolved with the edited sections on save, or null on cancel.</summary>
    public TaskCompletionSource<ReposSettingsEditResult?> Completion { get; } =
        new(TaskCreationOptions.RunContinuationsAsynchronously);
}
