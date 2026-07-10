using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Reads the OpenCode prompts shipped under <c>settings/opencode/prompts.json</c>.
/// Each prompt is a name plus prompt text loaded into the Start prompt box when
/// picked from the OpenCode panel selector.
/// </summary>
public interface IOpenCodePromptService
{
    /// <summary>
    /// Loads every prompt from the user-data <c>prompts.json</c>, seeding from the
    /// shipped default first. Never throws: malformed entries are logged and skipped.
    /// </summary>
    Task<IReadOnlyList<OpenCodePromptEntry>> LoadAsync();

    /// <summary>
    /// Saves a prompt with <paramref name="name"/> and <paramref name="prompt"/> to the
    /// user-data <c>prompts.json</c>, replacing any existing prompt with the same name
    /// (case-insensitive). Seeds the shipped default first when no user copy exists yet.
    /// Never throws: write errors are logged and swallowed.
    /// </summary>
    Task SaveAsync(string name, string prompt);
}
