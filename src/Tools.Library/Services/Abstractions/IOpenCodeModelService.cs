namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Loads the available opencode model list by running <c>opencode models</c> as a one-shot
/// process and parsing its stdout (one <c>provider/model-id</c> per line). This is the only
/// model source — the app no longer manages an <c>opencode serve</c> subprocess.
/// <para>
/// Every result — fresh or cached — respects the configured default model
/// (<see cref="Configuration.OpenCodeSettings.DefaultModel"/>): when one is set and the CLI
/// list does not contain it, it is prepended so the preselection always resolves and the
/// list is never empty while a default is configured.
/// </para>
/// </summary>
public interface IOpenCodeModelService
{
    /// <summary>
    /// Runs <c>&lt;executable&gt; models</c> and returns the printed model ids in the order
    /// opencode lists them. A non-empty result is persisted to the cache file for
    /// <see cref="GetCachedModels"/>. Every return path (including CLI failure and timeout)
    /// prepends <paramref name="defaultModel"/> when it is configured and not already
    /// listed, so a configured default keeps the selector usable even while the CLI is
    /// unavailable. Never throws.
    /// </summary>
    /// <param name="executable">OpenCode CLI path or command; a bare name is resolved
    /// through the user-level install folders as well as PATH.</param>
    /// <param name="defaultModel">Configured default model id, or null/empty for none.</param>
    Task<IReadOnlyList<string>> GetModelsAsync(string? executable, string? defaultModel, CancellationToken cancellationToken = default);

    /// <summary>
    /// Returns the model list persisted by the last successful <see cref="GetModelsAsync"/>
    /// call (under <c>%USERPROFILE%\.devtools\opencode</c>), or an empty list when no cache
    /// exists yet. <paramref name="defaultModel"/> is prepended when configured and not
    /// already listed. Lets callers show a usable list instantly while the CLI runs. Never
    /// throws.
    /// </summary>
    /// <param name="defaultModel">Configured default model id, or null/empty for none.</param>
    IReadOnlyList<string> GetCachedModels(string? defaultModel);
}
