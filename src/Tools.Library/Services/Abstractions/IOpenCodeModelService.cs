namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Loads the available opencode model list by running <c>opencode models</c> as a one-shot
/// process and parsing its stdout (one <c>provider/model-id</c> per line). This is the only
/// model source — the app no longer manages an <c>opencode serve</c> subprocess.
/// </summary>
public interface IOpenCodeModelService
{
    /// <summary>
    /// Runs <c>&lt;executable&gt; models</c> and returns the printed model ids in the order
    /// opencode lists them. A non-empty result is persisted to the cache file for
    /// <see cref="GetCachedModels"/>. Returns an empty list when the executable is missing,
    /// times out, or prints nothing; never throws.
    /// </summary>
    Task<IReadOnlyList<string>> GetModelsAsync(string? executable, CancellationToken cancellationToken = default);

    /// <summary>
    /// Returns the model list persisted by the last successful <see cref="GetModelsAsync"/>
    /// call (under <c>%USERPROFILE%\.devtools\opencode</c>), or an empty list when no cache
    /// exists yet. Lets callers show a usable list instantly while the CLI runs. Never throws.
    /// </summary>
    IReadOnlyList<string> GetCachedModels();
}
