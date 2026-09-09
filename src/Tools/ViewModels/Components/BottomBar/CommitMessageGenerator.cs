using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// The commit-message wand's mechanics, owned by <see cref="ChangesTabViewModel"/>
/// (which keeps the UI-facing flag and the commands): the in-flight generation's
/// cancellation scope, the prompt assembly and the answer cleanup.
/// <para>
/// The token rides into the run service, whose kill-on-cancel handler terminates the
/// opencode process tree the moment it fires — on a repo switch, app shutdown, or
/// generation teardown (cancel-before-dispose in <see cref="End"/>; the service kills
/// the tree itself on its own timeout) — so the CLI must not keep running in the
/// background.
/// </para>
/// </summary>
public sealed class CommitMessageGenerator
{
    private readonly IOpenCodeRunService _openCodeRunService;
    private readonly ICommitMessagePromptService _promptService;

    /// <summary>The in-flight message generation's cancellation source.</summary>
    private CancellationTokenSource? _cts;

    /// <summary>Upper bound on the staged diff fed to the model, so the prompt stays sane.</summary>
    private const int MaxPromptPatchLength = 8000;

    public CommitMessageGenerator(
        IOpenCodeRunService openCodeRunService,
        ICommitMessagePromptService promptService)
    {
        _openCodeRunService = openCodeRunService;
        _promptService = promptService;
    }

    /// <summary>Cancels any in-flight message generation. Safe to call anytime.</summary>
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
    /// The wand's core: runs the staged patch through opencode (the user-editable
    /// prompt template, the dedicated commit model or the default) and returns the
    /// cleaned message — null when nothing is staged, the CLI fails, or nothing usable
    /// came back. The caller reports the failure.
    /// </summary>
    /// <param name="stagedPaths">The staged file list snapshotted before the run — a
    /// repo switch mid-run reloads the caller's collections, and the prompt must not
    /// end up mixing the old diff with the new repo's file list and tone context.</param>
    /// <param name="recentSubjects">The repo's recent commit subjects (tone reference),
    /// snapshotted like the file list.</param>
    /// <param name="getStagedPatch">Fetches the staged diff for the repo.</param>
    /// <param name="executable">The configured opencode executable.</param>
    /// <param name="model">The model to run (the dedicated commit model or the default).</param>
    public async Task<string?> TryGenerateAsync(
        CancellationToken cancellationToken,
        Repo repo,
        string[] stagedPaths,
        string[] recentSubjects,
        Func<Repo, Task<string>> getStagedPatch,
        string executable,
        string? model)
    {
        var patch = await getStagedPatch(repo);
        if (string.IsNullOrWhiteSpace(patch)) return null;

        var prompt = BuildPrompt(patch, stagedPaths, recentSubjects);
        var answer = await _openCodeRunService.RunAsync(executable, model, prompt, cancellationToken);
        return CleanGeneratedMessage(answer);
    }

    /// <summary>
    /// Builds the wand's prompt: fills the user-editable template's placeholders with
    /// the staged file list, the (truncated) staged diff, and the repo's recent commit
    /// subjects as free-form context (tone reference). All three inputs are snapshots
    /// taken before the run started, never the live collections.
    /// </summary>
    private string BuildPrompt(string patch, string[] stagedPaths, string[] recentSubjects)
    {
        if (patch.Length > MaxPromptPatchLength)
        {
            patch = patch[..MaxPromptPatchLength] + "\n… (diff truncated)";
        }

        var fileList = string.Join(", ", stagedPaths);
        var context = recentSubjects.Length > 0
            ? "Recent commit subjects for tone:\n" + string.Join('\n', recentSubjects)
            : string.Empty;
        return _promptService.BuildPrompt(fileList, patch, context);
    }

    /// <summary>
    /// Normalizes the model's answer into a commit message: strips code fences and
    /// wrapping quotes, keeps the remaining lines (the template may emit subject +
    /// body + footer), and caps the total length. Returns null when nothing usable
    /// came back.
    /// </summary>
    private static string? CleanGeneratedMessage(string? answer)
    {
        if (string.IsNullOrWhiteSpace(answer)) return null;

        // Interior blank lines must survive (sections of a structured body are separated
        // by them), and so must a wrapped bullet's 2-space continuation indent — only
        // trailing whitespace, the fences and the outer padding are stripped.
        var lines = answer.Split('\n')
            .Select(l => l.TrimEnd())
            .SkipWhile(l => l.StartsWith("```", StringComparison.Ordinal) || l.Trim().Length == 0)
            .Reverse().SkipWhile(l => l.StartsWith("```", StringComparison.Ordinal) || l.Trim().Length == 0).Reverse()
            .ToList();
        var message = string.Join('\n', lines).Trim().Trim('"', '`').Trim();
        if (message.Length > 1500) message = message[..1500].TrimEnd();
        return message.Length == 0 ? null : message;
    }
}
