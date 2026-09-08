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
/// <para>
/// The catalog is memoized in memory for a short TTL: repeated drawer opens and quick
/// launches reuse it instead of re-running the CLI or re-reading the cache file; an
/// explicit refresh (<see cref="GetModelsAsync"/> with <c>forceRefresh</c>) bypasses the
/// memo and truly refreshes.
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
    /// <para>
    /// Within a short TTL the previous catalog answers without a CLI run (re-merged with
    /// the current default); only an expired catalog — or <paramref name="forceRefresh"/>,
    /// which must truly refresh — spawns the CLI. A failed or empty run leaves the memo
    /// untouched, so the next call retries.
    /// </para>
    /// </summary>
    /// <param name="executable">OpenCode CLI path or command; a bare name is resolved
    /// through the user-level install folders as well as PATH.</param>
    /// <param name="defaultModel">Configured default model id, or null/empty for none.</param>
    /// <param name="forceRefresh">Re-runs the CLI even when the in-memory catalog is still fresh.</param>
    Task<IReadOnlyList<string>> GetModelsAsync(string? executable, string? defaultModel, bool forceRefresh = false, CancellationToken cancellationToken = default);

    /// <summary>
    /// Returns the model list persisted by the last successful <see cref="GetModelsAsync"/>
    /// call (under <c>%USERPROFILE%\.devtools\opencode</c>), or an empty list when no cache
    /// exists yet. The file read is memoized for the same TTL as the CLI catalog (a CLI run
    /// in this process refreshes the memo directly, since it rewrites the file).
    /// <paramref name="defaultModel"/> is prepended when configured and not already listed.
    /// Lets callers show a usable list instantly while the CLI runs. Never throws.
    /// </summary>
    /// <param name="defaultModel">Configured default model id, or null/empty for none.</param>
    IReadOnlyList<string> GetCachedModels(string? defaultModel);

    /// <summary>
    /// Resolves the model a launch/preselection uses when the user has not picked one: the
    /// configured default when set — matched case-insensitively against
    /// <paramref name="models"/> and resolved to the list's own casing — otherwise the
    /// first model; empty when the list is empty and no default is configured. The single
    /// source of the rule the merge applies to the catalog's ordering (the configured
    /// default leads it), exposed so callers can resolve from a list they already hold.
    /// </summary>
    /// <param name="models">The model catalog as returned by this service.</param>
    /// <param name="defaultModel">Configured default model id, or null/empty for none.</param>
    string ResolveLaunchModel(IReadOnlyList<string> models, string? defaultModel);
}
