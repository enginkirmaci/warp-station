using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Loads the daily-summary prompt template from the user's opencode folder and fills
/// its placeholders. Template storage follows <see cref="UserPromptTemplateService"/>
/// (user-editable <c>daily-summary.md</c>, seeded from the shipped default, built-in
/// constant as last resort). Re-read on every wand run — no caching.
/// </summary>
public class DailySummaryPromptService : UserPromptTemplateService, IDailySummaryPromptService
{
    /// <summary>Placeholder tokens the template must carry.</summary>
    private const string RepoNameToken = "{repo_name}";
    private const string BranchToken = "{branch}";
    private const string DateToken = "{date}";
    private const string CommitLogToken = "{commit_log}";
    private const string WorkingChangesToken = "{working_changes}";

    /// <summary>
    /// The built-in template (also the source of the shipped default file): a
    /// "what did I do yesterday / where did I leave off" report writer fed with the
    /// day's commits and the still-uncommitted working-tree state.
    /// </summary>
    private const string DefaultTemplate = """
        You are a daily work-summary writer. Given one repository's commits and still-uncommitted changes from a single day, write a short markdown report that tells the developer what they did that day and where they left off.

        FORMAT:
        # <repository> — work summary, <date>

        ## What was done
        (one bullet per commit, or per coherent group of related commits)

        ## Work in progress
        (the uncommitted changes — what is half-done right now; omit the section when there are none)

        ## Where you left off
        (2-3 sentences describing the exact state the day ended in)

        ## Suggested next steps
        (2-4 concrete bullets that follow from the state above)

        RULES:
        - Ground every statement in the commit subjects and the changed-file list — never invent work, decisions, or outcomes
        - Group related commits into one bullet when they clearly belong together; never quote subjects verbatim
        - Keep the whole report under ~40 lines; no tables, no horizontal rules
        - Output ONLY the markdown report — no wrapping code fences, no explanation

        INPUT:
        Repository: {repo_name}
        Branch: {branch}
        Day: {date}

        Commits made that day:
        {commit_log}

        Uncommitted changes (current working tree — may also contain newer edits):
        {working_changes}
        """;

    public DailySummaryPromptService() : base("daily-summary.md", "opencode/daily-summary.md", DefaultTemplate)
    {
    }

    public string BuildPrompt(string repoName, string branch, string dateLabel, string commitLog, string workingChanges)
    {
        return LoadTemplate()
            .Replace(RepoNameToken, repoName)
            .Replace(BranchToken, branch)
            .Replace(DateToken, dateLabel)
            .Replace(CommitLogToken, commitLog)
            .Replace(WorkingChangesToken, workingChanges);
    }
}
