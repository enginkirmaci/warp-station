using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Loads the OpenCode model list from <c>settings/opencode/models.json</c>.
/// </summary>
public interface IOpenCodeModelService
{
    /// <summary>
    /// Loads the model list, falling back to built-in defaults when the file is
    /// missing, unreadable, or contains no models.
    /// </summary>
    Task<OpenCodeModelsConfig> LoadAsync();
}
