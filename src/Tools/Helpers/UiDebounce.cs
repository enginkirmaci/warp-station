namespace Tools.Helpers;

/// <summary>
/// Coalesces a burst of calls into a single callback after an idle window: every
/// <see cref="Debounce"/> call cancels the still-pending callback and restarts the
/// window, so only the last action of a burst runs (marshaled onto the UI thread).
/// Shared by the header search push and the Repos page's filter / service-changed
/// rebuilds, replacing their per-site copies of the
/// CTS + Task.Delay + Dispatcher.Post pattern.
/// </summary>
/// <remarks>
/// UI-thread affinity: the single <see cref="_cts"/> field is written only from
/// UI-thread handlers (text-changed, navigation, window-closed), so no locking is
/// needed. The source is swapped per call and never disposed there — disposing one
/// whose <see cref="Task.Delay"/> timer may still be registered races an
/// <see cref="ObjectDisposedException"/> inside the delay task (the bug in the code
/// this class replaces). <see cref="Cancel"/> already unregisters that timer, so the
/// abandoned source holds nothing live and is left to the GC; a real dispose happens
/// only on owner teardown via <see cref="Dispose"/>.
/// </remarks>
public sealed class UiDebounce : IDisposable
{
    /// <summary>Idle window (milliseconds) before the action runs.</summary>
    private readonly int _delayMs;

    private CancellationTokenSource? _cts;

    /// <param name="delayMs">Idle window (milliseconds) before the action runs.</param>
    public UiDebounce(int delayMs) => _delayMs = delayMs;

    /// <summary>Cancels the pending callback without restarting the idle window.</summary>
    public void Cancel() => _cts?.Cancel();

    /// <summary>
    /// Restarts the idle window. <paramref name="action"/> runs on the UI thread once
    /// the window elapses without another call; a canceled burst runs nothing.
    /// </summary>
    public void Debounce(Action action)
    {
        _cts?.Cancel();
        // Fresh source per burst; the previous one is NOT disposed here — Cancel above
        // has already unregistered its Task.Delay, so nothing is left to clean up (see
        // the class remarks for the dispose race this avoids).
        var cts = new CancellationTokenSource();
        _cts = cts;
        var token = cts.Token;
        _ = Task.Run(async () =>
        {
            try
            {
                await Task.Delay(_delayMs, token);
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    if (token.IsCancellationRequested) return;
                    action();
                });
            }
            catch (OperationCanceledException) { }
        });
    }

    /// <summary>
    /// Owner teardown (window closed / page detached): cancels the pending callback so
    /// its UI-thread work never fires. Cancel runs the delay's registration
    /// synchronously, so by the time it returns no Task.Delay is registered on the
    /// source anymore and disposing it cannot race the delay's timer.
    /// </summary>
    public void Dispose()
    {
        _cts?.Cancel();
        _cts?.Dispose();
        _cts = null;
    }
}
