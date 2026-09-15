using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// Shared mechanics of the wand generators (commit message, README): the in-flight
/// run's cancellation scope and the opencode call with its answer cleanup. The token
/// rides into the run service, whose kill-on-cancel handler terminates the opencode
/// process tree the moment it fires — on a repo switch, app shutdown, or generation
/// teardown (cancel-before-dispose in <see cref="End"/>; the service kills the tree
/// itself on its own timeout) — so the CLI must not keep running in the background.
/// </summary>
public abstract class OpenCodeWandGenerator
{
    private readonly IOpenCodeRunService _openCodeRunService;

    /// <summary>The in-flight generation's cancellation source.</summary>
    private CancellationTokenSource? _cts;

    protected OpenCodeWandGenerator(IOpenCodeRunService openCodeRunService)
        => _openCodeRunService = openCodeRunService;

    /// <summary>Cancels any in-flight generation. Safe to call anytime.</summary>
    public void Cancel()
    {
        try { _cts?.Cancel(); }
        catch (ObjectDisposedException) { /* the owning command already tore it down */ }
    }

    /// <summary>Supersedes any stray previous source and opens a new cancellation scope.</summary>
    public CancellationToken Begin()
    {
        Cancel();
        _cts?.Dispose();
        _cts = new CancellationTokenSource();
        return _cts.Token;
    }

    public void End()
    {
        // Cancel before dispose: after a timed-out run the CTS is the only remaining
        // hook to the kill-on-cancel registration in OpenCodeRunService — disposing it
        // silently would leave a stray Electron tree running forever.
        try { _cts?.Cancel(); }
        catch (ObjectDisposedException) { /* a superseding scope already tore it down */ }
        _cts?.Dispose();
        _cts = null;
    }

    /// <summary>
    /// Runs the prompt through opencode and normalizes the answer. Returns null when
    /// nothing usable came back.
    /// </summary>
    protected async Task<string?> RunAsync(
        CancellationToken cancellationToken,
        string executable,
        string? model,
        string prompt,
        int maxAnswerLength)
    {
        var answer = await _openCodeRunService.RunAsync(executable, model, prompt, cancellationToken);
        return CleanAnswer(answer, maxAnswerLength);
    }

    /// <summary>
    /// Normalizes a model answer: strips code fences and wrapping quotes, keeps the
    /// remaining lines (interior blank lines survive — sections of a structured body
    /// are separated by them), and caps the total length. Null when nothing usable
    /// came back.
    /// </summary>
    private static string? CleanAnswer(string? answer, int maxLength)
    {
        if (string.IsNullOrWhiteSpace(answer)) return null;

        var lines = answer.Split('\n')
            .Select(l => l.TrimEnd())
            .SkipWhile(l => l.StartsWith("```", StringComparison.Ordinal) || l.Trim().Length == 0)
            .Reverse().SkipWhile(l => l.StartsWith("```", StringComparison.Ordinal) || l.Trim().Length == 0).Reverse()
            .ToList();
        var message = string.Join('\n', lines).Trim().Trim('"', '`').Trim();
        if (message.Length > maxLength) message = message[..maxLength].TrimEnd();
        return message.Length == 0 ? null : message;
    }
}
