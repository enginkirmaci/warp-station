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
    /// opencode lists them. Returns an empty list when the executable is missing, times out,
    /// or prints nothing; never throws.
    /// </summary>
    Task<IReadOnlyList<string>> GetModelsAsync(string? executable, CancellationToken cancellationToken = default);
}
