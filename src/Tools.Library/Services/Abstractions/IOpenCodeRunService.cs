namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Runs the opencode CLI in its one-shot mode (<c>opencode run</c>): sends one prompt,
/// captures the model's answer, exits. Powers the Changes tab's wand button (generate a
/// commit message from the staged diff). Distinct from <see cref="IOpenCodeModelService"/>,
/// which only lists models.
/// </summary>
public interface IOpenCodeRunService
{
    /// <summary>
    /// Runs one prompt through opencode and returns its stdout answer (trimmed), or
    /// null on any failure (missing CLI, non-zero exit, timeout). An empty
    /// <paramref name="model"/> lets opencode pick its own default. Never throws.
    /// </summary>
    Task<string?> RunAsync(string? executable, string? model, string prompt, CancellationToken cancellationToken = default);
}
