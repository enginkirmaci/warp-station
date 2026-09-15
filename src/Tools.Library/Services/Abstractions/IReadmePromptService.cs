namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Builds the README wand's prompt from the user-editable template at
/// <c>%USERPROFILE%\.devtools\opencode\readme.md</c> (the built-in default materializes
/// the file on first run). The template's <c>{repo_name}</c>, <c>{file_tree}</c> and
/// <c>{context}</c> placeholders are filled per run.
/// </summary>
public interface IReadmePromptService
{
    /// <summary>
    /// The resolved prompt: the template with every placeholder replaced. Falls back to
    /// the built-in default text when neither the user's file nor the shipped one exists.
    /// </summary>
    string BuildPrompt(string repoName, string fileTree, string context);
}
