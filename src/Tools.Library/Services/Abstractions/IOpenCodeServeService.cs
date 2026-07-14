using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Owns the managed <c>opencode serve</c> subprocess and the registry of launched opencode
/// instances. A singleton: the status bar and the Repos page both observe it. The serve
/// process is started on demand (<see cref="EnsureStartedAsync"/>) and stopped on app
/// shutdown (<see cref="Stop"/>).
/// </summary>
public interface IOpenCodeServeService
{
    /// <summary>True when the serve server has answered a health check recently.</summary>
    bool IsConnected { get; }

    /// <summary>
    /// Raised on the connection state changing to <see cref="IsConnected"/>. Handlers should
    /// marshal to the UI thread themselves (callers do this already via <c>Dispatcher.UIThread.Post</c>).
    /// </summary>
    event EventHandler<bool>? ConnectionChanged;

    /// <summary>
    /// Raised whenever a registered instance's status or auto-approve flag changes. The
    /// sender is the affected <see cref="OpenCodeInstance"/>. Used to mirror state onto the
    /// matching <see cref="Repo"/>.
    /// </summary>
    event Action<OpenCodeInstance>? InstanceChanged;

    /// <summary>
    /// Applies the serve connection settings and, if <see cref="OpenCodeServeSettings.AutoConnect"/>
    /// is enabled (or <paramref name="force"/> is true), starts the serve subprocess and begins
    /// the connection-health poll and the global event listener. Idempotent: a no-op if already
    /// running. Safe to call from the UI thread on every Repos navigation.
    /// </summary>
    Task EnsureStartedAsync(OpenCodeServeSettings settings, bool force = false, CancellationToken cancellationToken = default);

    /// <summary>
    /// Stops the serve subprocess and clears connection state. Idempotent.
    /// </summary>
    void Stop();

    /// <summary>
    /// Registers a freshly launched opencode instance for a repo folder, so the service can
    /// match it to a serve session, track its status, and (when <paramref name="autoApprove"/>
    /// is true) auto-approve its permission requests. Re-registering the same folder updates
    /// the entry (e.g. a new launch replaces a stopped instance).
    /// </summary>
    /// <returns>The registered (or updated) instance.</returns>
    OpenCodeInstance Register(string folderPath, int? pid, bool autoApprove);

    /// <summary>Removes a repo's instance from the registry (e.g. user cleared it).</summary>
    void Unregister(string folderPath);

    /// <summary>Looks up the tracked instance for a repo folder, if any.</summary>
    OpenCodeInstance? GetInstance(string folderPath);

    /// <summary>Toggles a registered instance's auto-approve at runtime.</summary>
    void SetAutoApprove(string folderPath, bool value);
}
