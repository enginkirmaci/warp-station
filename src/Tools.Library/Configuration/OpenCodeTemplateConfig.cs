namespace Tools.Library.Configuration;

/// <summary>
/// The contents of a template's <c>template.json</c>: metadata shown in the
/// OpenCode template selector. The prompt is managed separately in
/// <c>settings/opencode/prompts.json</c>.
/// </summary>
public sealed class OpenCodeTemplateConfig
{
    /// <summary>
    /// Display name shown in the template drop-down. Falls back to the folder
    /// name when empty.
    /// </summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>
    /// Short description rendered under the selector. May be empty.
    /// </summary>
    public string Description { get; set; } = string.Empty;
}

/// <summary>
/// A resolved template bound to the OpenCode panel: the deserialized metadata
/// plus the source folder to copy into a repo as <c>.opencode</c>.
/// </summary>
public sealed class OpenCodeTemplate
{
    /// <summary>
    /// Sentinel representing the optional "no template" choice. Has an empty
    /// <see cref="FolderPath"/> so the copy step is skipped.
    /// </summary>
    public static readonly OpenCodeTemplate None = new()
    {
        Name = "(None)",
        Description = string.Empty,
        FolderPath = string.Empty,
    };

    /// <summary>Display name from <c>template.json</c> (or the folder name).</summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>Short description from <c>template.json</c>.</summary>
    public string Description { get; set; } = string.Empty;

    /// <summary>
    /// Absolute path to the template source folder under
    /// <c>%USERPROFILE%\.devtools\settings\opencode\templates\&lt;name&gt;</c>. Empty for
    /// <see cref="None"/>.
    /// </summary>
    public string FolderPath { get; set; } = string.Empty;

    /// <summary>True when this is the <see cref="None"/> sentinel (no copy performed).</summary>
    public bool IsNone => string.IsNullOrEmpty(FolderPath);

    /// <summary>The name shown by the drop-down.</summary>
    public override string ToString() => Name;
}
