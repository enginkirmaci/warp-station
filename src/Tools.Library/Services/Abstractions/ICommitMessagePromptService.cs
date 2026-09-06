namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Builds the wand's commit-message prompt from the user-editable template at
/// <c>%USERPROFILE%\.devtools\settings\commit-message.md</c> (seeded from the shipped
/// default on first run). The template's <c>{file_list}</c>, <c>{diff}</c> and
/// <c>{context}</c> placeholders are filled per run.
/// </summary>
public interface ICommitMessagePromptService
{
    /// <summary>
    /// The resolved prompt: the template with every placeholder replaced. Falls back to
    /// the built-in default text when neither the user's file nor the shipped one exists.
    /// </summary>
    string BuildPrompt(string fileList, string diff, string context);
}
