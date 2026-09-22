namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Builds the daily-summary wand's prompt from the user-editable template at
/// <c>~/.devtools/opencode/daily-summary.md</c> (the built-in default materializes
/// the file on first run). The template's <c>{repo_name}</c>, <c>{branch}</c>,
/// <c>{date}</c>, <c>{commit_log}</c> and <c>{working_changes}</c> placeholders are
/// filled per run.
/// </summary>
public interface IDailySummaryPromptService
{
    /// <summary>
    /// The resolved prompt: the template with every placeholder replaced. Falls back to
    /// the built-in default text when neither the user's file nor the shipped one exists.
    /// </summary>
    string BuildPrompt(string repoName, string branch, string dateLabel, string commitLog, string workingChanges);
}
