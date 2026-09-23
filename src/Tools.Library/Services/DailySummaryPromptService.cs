using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Loads the daily-summary prompt template from the user's opencode folder and fills
/// its placeholders. Template storage follows <see cref="UserPromptTemplateService"/>
/// (user-editable <c>daily-summary-all.md</c>, seeded from the shipped default,
/// built-in constant as last resort). Re-read on every wand run — no caching.
/// </summary>
public class DailySummaryPromptService : UserPromptTemplateService, IDailySummaryPromptService
{
    /// <summary>Placeholder tokens the template must carry.</summary>
    private const string DateToken = "{date}";
    private const string RepositoriesToken = "{repos}";

    /// <summary>
    /// The built-in template (also the source of the shipped default file): a
    /// "what did I do yesterday / where did I leave off" report writer fed with every
    /// active repository's commits and still-uncommitted working-tree state.
    /// </summary>
    private const string DefaultTemplate = """
        You are a daily work-summary writer. Given several repositories' commits and still-uncommitted changes from a single day, write one short markdown report that tells the developer what they did that day across their projects and where each effort left off.

        FORMAT:
        # Work summary, <date>

        ## What was done
        (one bullet per repository that had commits, or per coherent group of related commits; start each bullet with the repository name in bold)

        ## Work in progress
        (one bullet per repository with uncommitted changes — what is half-done right now; omit the section when there are none)

        ## Where you left off
        (2-3 sentences per active repository, or one paragraph covering the day when the efforts intertwine)

        ## Suggested next steps
        (2-5 concrete bullets that follow from the state above)

        RULES:
        - Ground every statement in the commit subjects and the changed-file lists — never invent work, decisions, or outcomes
        - Group related commits into one bullet when they clearly belong together; never quote subjects verbatim
        - Keep the whole report under ~60 lines; no tables, no horizontal rules
        - Output ONLY the markdown report — no wrapping code fences, no explanation

        INPUT:
        Day: {date}

        Repository activity:
        {repos}
        """;

    public DailySummaryPromptService() : base("daily-summary-all.md", "opencode/daily-summary-all.md", DefaultTemplate)
    {
    }

    public string BuildPrompt(string dateLabel, string repositoriesBlock)
    {
        return LoadTemplate()
            .Replace(DateToken, dateLabel)
            .Replace(RepositoriesToken, repositoriesBlock);
    }
}
