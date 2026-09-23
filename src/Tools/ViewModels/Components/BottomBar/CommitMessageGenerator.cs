using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// The commit-message wand's prompt building, owned by <see cref="ChangesTabViewModel"/>
/// (which keeps the UI-facing flag and the commands); the cancellation scope, the
/// opencode run and the answer cleanup live in <see cref="OpenCodeWandGenerator"/>.
/// </summary>
public sealed class CommitMessageGenerator : OpenCodeWandGenerator
{
    private readonly ICommitMessagePromptService _promptService;

    /// <summary>Upper bound on the staged diff fed to the model, so the prompt stays sane.</summary>
    private const int MaxPromptPatchLength = 8000;

    public CommitMessageGenerator(
        IOpenCodeRunService openCodeRunService,
        ICommitMessagePromptService promptService)
        : base(openCodeRunService)
        => _promptService = promptService;

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
        if (string.IsNullOrWhiteSpace(patch))
        {
            Serilog.Log.Debug("Commit-message wand: staged patch for {Path} is empty — nothing to generate from", repo.FolderPath);
            return null;
        }

        var prompt = BuildPrompt(patch, stagedPaths, recentSubjects);
        return await RunAsync(cancellationToken, executable, model, prompt, maxAnswerLength: 1500);
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
}
