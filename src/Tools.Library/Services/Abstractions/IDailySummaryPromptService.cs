namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Builds the daily-summary wand's prompt: fills the user-editable template
/// (<c>~/.devtools/opencode/daily-summary-all.md</c>) with the day label and the
/// per-repository activity block the generator shaped.
/// </summary>
public interface IDailySummaryPromptService
{
    /// <summary>The resolved prompt: the template with every placeholder replaced.
    /// Falls back to the built-in default text when neither the user's file nor the
    /// shipped one exists.</summary>
    string BuildPrompt(string dateLabel, string repositoriesBlock);
}
