namespace Tools.Helpers;

/// <summary>
/// Coalesces a burst of calls into a single UI-thread callback: while one posted pass
/// is still queued, further calls are dropped entirely — the queued run sees the
/// latest state when it executes, so running it once per burst is enough. Replaces
/// the per-site copies of the posted-flag + <c>Dispatcher.UIThread.Post</c> idiom
/// (the bar's repo-dropdown rebuild, the OpenCode model list rebuild).
/// </summary>
/// <remarks>
/// Thread affinity is loose by design: <see cref="Post"/> may be called from background
/// handlers (the repo service raises <c>Changed</c> off the UI thread), so the posted
/// flag is a plain bool without locking — a torn read only duplicates one callback,
/// which the coalescing already tolerates; it can never lose one, because a call that
/// misses the flag posts a fresh pass.
/// </remarks>
public sealed class UiPostOnce
{
    private bool _posted;

    /// <summary>
    /// Posts <paramref name="action"/> onto the UI thread once per burst: a call while
    /// an earlier post is still queued does nothing.
    /// </summary>
    public void Post(Action action)
    {
        if (_posted) return;
        _posted = true;
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            _posted = false;
            action();
        });
    }
}
