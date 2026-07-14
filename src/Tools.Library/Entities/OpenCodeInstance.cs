using CommunityToolkit.Mvvm.ComponentModel;

namespace Tools.Library.Entities;

/// <summary>
/// A tracked opencode instance: one launched for a specific repo folder. Held in the
/// <c>IOpenCodeServeService</c> registry keyed by <see cref="FolderPath"/>. The serve
/// service keeps <see cref="Status"/>, <see cref="SessionId"/> and <see cref="AutoApprove"/>
/// up to date as it matches serve sessions and processes approval events.
/// </summary>
public partial class OpenCodeInstance : ObservableObject
{
    /// <summary>
    /// The repo folder this instance was launched in. Also the registry key and the value
    /// used to match a serve <c>Session.directory</c>.
    /// </summary>
    public string FolderPath { get; set; } = string.Empty;

    /// <summary>
    /// The opencode/terminal process id, when known. The legacy launcher is fire-and-forget
    /// (<c>UseShellExecute</c>), so this may be <see langword="null"/>; in that case the
    /// instance is matched to a serve session by working directory instead.
    /// </summary>
    public int? Pid { get; set; }

    /// <summary>
    /// The opencode serve session id once the instance has been matched to a session
    /// (via <c>GET /session</c> by <see cref="FolderPath"/>). <see langword="null"/> until matched.
    /// </summary>
    public string? SessionId { get; set; }

    /// <summary>
    /// When auto-approve is on for this instance, the serve service replies
    /// <c>"once"</c> to every <c>permission.updated</c> event whose session matches.
    /// </summary>
    [ObservableProperty]
    private bool _autoApprove;

    /// <summary>
    /// The live lifecycle status, mirrored onto the owning <see cref="Repo.OpenCodeStatus"/>.
    /// </summary>
    [ObservableProperty]
    private OpenCodeInstanceStatus _status = OpenCodeInstanceStatus.Starting;

    /// <summary>The launch time, for display/diagnostics.</summary>
    public DateTimeOffset? StartedAt { get; set; }

    partial void OnAutoApproveChanged(bool value)
        => AutoApproveChanged?.Invoke(this, value);

    partial void OnStatusChanged(OpenCodeInstanceStatus value)
        => StatusChanged?.Invoke(this, value);

    /// <summary>Raised when <see cref="AutoApprove"/> changes.</summary>
    public event Action<OpenCodeInstance, bool>? AutoApproveChanged;

    /// <summary>Raised when <see cref="Status"/> changes.</summary>
    public event Action<OpenCodeInstance, OpenCodeInstanceStatus>? StatusChanged;
}
