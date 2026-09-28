namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Runs one wand prompt through opencode and returns the model's answer text. Two
/// implementations: <see cref="Tools.Library.Services.OpenCode.OpenCodeServeRunService"/>
/// (the default — drives an <c>opencode serve</c> process per run so tool-permission
/// asks surface in the app's popup) and <see cref="Tools.Library.Services.OpenCode.OpenCodeRunService"/>
/// (the one-shot <c>opencode run</c> CLI, whose asks are auto-denied — kept for the
/// README wand). Distinct from <see cref="IOpenCodeModelService"/>, which only lists models.
/// </summary>
public interface IOpenCodeRunService
{
    /// <summary>
    /// Runs one prompt through opencode and returns the model's answer (trimmed), or
    /// null on any failure (missing CLI, server error, timeout). An empty
    /// <paramref name="model"/> lets opencode pick its own default. Never throws.
    /// </summary>
    /// <param name="executable">The configured opencode executable.</param>
    /// <param name="model">The model id (catalog form, e.g. "provider/model").</param>
    /// <param name="prompt">The full prompt text.</param>
    /// <param name="cancellationToken">Cancels the run and kills the opencode
    /// process tree.</param>
    /// <param name="workingDirectory">Directory the opencode process runs in (the
    /// session's project scope); defaults to the app's working directory.</param>
    Task<string?> RunAsync(
        string? executable,
        string? model,
        string prompt,
        CancellationToken cancellationToken = default,
        string? workingDirectory = null);

    /// <summary>Kills every in-flight run's process tree. App shutdown calls this so a
    /// wand whose owning ViewModel is already unreachable (transient drawer hosts)
    /// cannot leave a stray Electron session behind.</summary>
    void Stop();
}
