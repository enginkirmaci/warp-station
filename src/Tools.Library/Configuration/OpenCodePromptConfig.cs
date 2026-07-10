namespace Tools.Library.Configuration;

/// <summary>
/// The contents of <c>settings/opencode/prompts.json</c>: the prompts offered in
/// the OpenCode prompt selector.
/// </summary>
public sealed class OpenCodePromptConfig
{
    /// <summary>
    /// The prompts shown in the OpenCode prompt selector, in display order.
    /// </summary>
    public List<OpenCodePromptEntry> Prompts { get; set; } = new();
}

/// <summary>
/// A single named prompt shown in the OpenCode prompt selector. Picking one
/// loads its <see cref="Prompt"/> into the Start prompt box, which the user can
/// still edit before launching.
/// </summary>
public sealed class OpenCodePromptEntry
{
    /// <summary>
    /// Sentinel representing the optional "no prompt" choice. Selected by default
    /// so the selector never auto-fills the Start prompt box.
    /// </summary>
    public static readonly OpenCodePromptEntry None = new()
    {
        Name = "(None)",
        Prompt = string.Empty,
    };

    /// <summary>Display name shown in the prompt drop-down.</summary>
    public string Name { get; set; } = string.Empty;

    /// <summary>
    /// The prompt text loaded into the Start prompt box when this entry is picked.
    /// </summary>
    public string Prompt { get; set; } = string.Empty;

    /// <summary>True when this is the <see cref="None"/> sentinel.</summary>
    public bool IsNone => ReferenceEquals(this, None);

    /// <summary>The name shown by the drop-down.</summary>
    public override string ToString() => Name;
}
