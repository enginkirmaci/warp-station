#if !WINDOWS
using Serilog;
using Tmds.DBus;

namespace Tools.Library.Services;

// Hand-written proxies for the freedesktop GlobalShortcuts portal. Tmds.DBus strips a
// trailing "Async" from method names (CreateSessionAsync → CreateSession), maps a{sv}
// to IDictionary<string, object>, (ussa{sv}) to the ValueTuple array, and signals to
// Watch<Member>Async methods taking a single-argument Action<T> whose T is the whole
// signal argument list as a ValueTuple (multi-arg Action<> is not supported). The
// interfaces must be public — Tmds.DBus Reflection.Emit's proxy cannot implement an
// inaccessible one.
[DBusInterface("org.freedesktop.portal.GlobalShortcuts")]
public interface IGlobalShortcutsPortal : IDBusObject
{
    Task<ObjectPath> CreateSessionAsync(IDictionary<string, object> options);
    // GlobalShortcuts v1 shortcut entry: (id, properties) with "description" and
    // "preferred_trigger" inside the properties dict. Newer portal versions switch to
    // (u version, s id, s description, a{sv}) — adapt if a machine rejects this shape.
    Task<ObjectPath> BindShortcutsAsync(ObjectPath sessionHandle,
        (string Id, IDictionary<string, object> Properties)[] shortcuts,
        string parentWindow, IDictionary<string, object> options);
}

[DBusInterface("org.freedesktop.portal.Request")]
public interface IPortalRequest : IDBusObject
{
    Task<IDisposable> WatchResponseAsync(Action<(uint Code, IDictionary<string, object> Results)> handler, Action<Exception>? onError = null);
}

[DBusInterface("org.freedesktop.portal.GlobalShortcuts")]
public interface IGlobalShortcutsSession : IDBusObject
{
    Task<IDisposable> WatchActivatedAsync(Action<(ObjectPath Session, string ShortcutId, ulong Timestamp, IDictionary<string, object> Options)> handler, Action<Exception>? onError = null);
}

[DBusInterface("org.freedesktop.portal.Session")]
public interface IPortalSession : IDBusObject
{
    Task CloseAsync();
}

/// <summary>
/// Registers a global hotkey (Ctrl+Shift+V) through the freedesktop GlobalShortcuts
/// portal (<c>org.freedesktop.portal.GlobalShortcuts</c>) — the only sanctioned way to
/// get compositor-level shortcuts on Wayland. The compositor asks the user to confirm
/// the binding once (the BindShortcuts dialog); afterwards <c>Activated</c> signals are
/// delivered for presses in any focused window, Wayland-native or XWayland.
/// Requires a portal backend implementing the interface (xdg-desktop-portal-hyprland,
/// -kde, -gnome, …); when it is missing the caller falls back to X11 grabs.
/// </summary>
internal sealed class GlobalShortcutsPortalHotkey : IDisposable
{
    public const string ShortcutId = "devtools-copy-password";

    private const string PortalService = "org.freedesktop.portal.Desktop";
    private const string PortalPath = "/org/freedesktop/portal/desktop";

    private Connection? _connection;
    private IDisposable? _activatedSubscription;
    private string _sessionPath = string.Empty;
    private bool _disposed;

    /// <summary>Raised on a D-Bus thread for each Activated signal of our shortcut.</summary>
    public event Action? HotkeyPressed;

    /// <summary>
    /// Connects to the session bus, creates a portal session and binds the shortcut.
    /// Returns false when the portal is unavailable, fails, or the user refuses the
    /// binding. <paramref name="bindTimeout"/> bounds the BindShortcuts round trip,
    /// which includes the user-facing confirmation dialog.
    /// </summary>
    public async Task<bool> TryRegisterAsync(TimeSpan? bindTimeout = null)
    {
        try
        {
            var connection = new Connection(Address.Session!);
            var info = await connection.ConnectAsync();
            _connection = connection;

            var portal = connection.CreateProxy<IGlobalShortcutsPortal>(PortalService, PortalPath);

            // --- CreateSession: no UI involved. The Response watch must be in place
            // BEFORE the call (portal spec race rule), so predict the request handle
            // path from our unique bus name + handle token, exactly as the portal does. ---
            var sender = info.LocalName.TrimStart(':').Replace('.', '_');
            var createToken = NewToken("create");
            var (createCode, createResults) = await PortalRequestAsync(connection, sender, createToken,
                call: () => portal.CreateSessionAsync(new Dictionary<string, object>
                {
                    ["handle_token"] = createToken,
                    ["session_handle_token"] = "devtools",
                }),
                timeout: TimeSpan.FromSeconds(10));
            _sessionPath = (string)createResults["session_handle"];
            Log.Logger.Debug("GlobalShortcuts portal: session created at {SessionPath}", _sessionPath);

            // --- BindShortcuts: the compositor may show its confirmation dialog here. ---
            var bindToken = NewToken("bind");
            _ = await PortalRequestAsync(connection, sender, bindToken,
                call: () => portal.BindShortcutsAsync(
                    _sessionPath,
                    new (string, IDictionary<string, object>)[]
                    {
                        (ShortcutId, new Dictionary<string, object>
                        {
                            ["description"] = "Copy stored password to the clipboard",
                            ["preferred_trigger"] = "CTRL+SHIFT+V",
                        }),
                    },
                    string.Empty,
                    new Dictionary<string, object> { ["handle_token"] = bindToken }),
                timeout: bindTimeout ?? TimeSpan.FromSeconds(120));

            // --- Activated: delivered by the compositor for every accepted press. ---
            var sessionProxy = connection.CreateProxy<IGlobalShortcutsSession>(PortalService, _sessionPath);
            _activatedSubscription = await sessionProxy.WatchActivatedAsync(OnActivated);

            return true;
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "GlobalShortcuts portal: could not register the Ctrl+Shift+V binding");
            Dispose();
            return false;
        }
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;

        _activatedSubscription?.Dispose();
        _activatedSubscription = null;

        var connection = _connection;
        _connection = null;
        if (connection is null)
        {
            return;
        }

        try
        {
            // Ask the portal to drop the session so the compositor releases the binding.
            if (!string.IsNullOrEmpty(_sessionPath))
            {
                connection.CreateProxy<IPortalSession>(PortalService, _sessionPath)
                    .CloseAsync().Wait(TimeSpan.FromSeconds(2));
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Debug(ex, "GlobalShortcuts portal: closing the session failed");
        }

        connection.Dispose();
    }

    private void OnActivated((ObjectPath Session, string ShortcutId, ulong Timestamp, IDictionary<string, object> Options) signal)
    {
        if (signal.ShortcutId == ShortcutId)
        {
            HotkeyPressed?.Invoke();
        }
    }

    /// <summary>
    /// The common portal round trip: watch Response on the request handle path derived
    /// from our unique name and token, issue the call, verify the returned handle
    /// matches, then await the outcome.
    /// </summary>
    private static async Task<(uint Code, IDictionary<string, object> Results)> PortalRequestAsync(
        Connection connection, string sender, string token,
        Func<Task<ObjectPath>> call, TimeSpan timeout)
    {
        var expectedRequestPath = $"/org/freedesktop/portal/desktop/request/{sender}/{token}";
        var requestProxy = connection.CreateProxy<IPortalRequest>(PortalService, expectedRequestPath);

        var completion = new TaskCompletionSource<(uint, IDictionary<string, object>)>(TaskCreationOptions.RunContinuationsAsynchronously);
        void OnResponse((uint Code, IDictionary<string, object> Results) signal)
        {
            if (signal.Code == 0)
            {
                completion.TrySetResult(signal);
            }
            else
            {
                completion.TrySetException(new InvalidOperationException($"portal request rejected (response code {signal.Code})"));
            }
        }

        var subscription = await requestProxy.WatchResponseAsync(OnResponse);
        try
        {
            var returnedPath = (await call().WaitAsync(timeout)).ToString();
            if (returnedPath != expectedRequestPath)
            {
                throw new InvalidOperationException($"portal returned an unexpected request handle: {returnedPath}");
            }

            return await completion.Task.WaitAsync(timeout);
        }
        finally
        {
            subscription.Dispose();
        }
    }

    private static string NewToken(string kind)
        => $"devtools_{kind}_{Guid.NewGuid().ToString("N")[..8]}";
}
#endif
