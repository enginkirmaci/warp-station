namespace Tools.Library.Configuration;

/// <summary>
/// The contents of <c>settings/opencode/models.json</c>: the models offered in the
/// OpenCode model selector and the one selected by default.
/// </summary>
public sealed class OpenCodeModelsConfig
{
    /// <summary>
    /// The model id selected when the OpenCode panel opens. If empty, the first entry
    /// in <see cref="Models"/> is used.
    /// </summary>
    public string DefaultModel { get; set; } = string.Empty;

    /// <summary>
    /// The model ids shown in the OpenCode model selector, in display order.
    /// </summary>
    public List<string> Models { get; set; } = new();
}
