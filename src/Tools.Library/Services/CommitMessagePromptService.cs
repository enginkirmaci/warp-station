using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Loads the commit-message prompt template from the user's opencode folder and fills
/// its placeholders. Template storage follows <see cref="UserPromptTemplateService"/>
/// (user-editable <c>commit-message.md</c>, seeded from the shipped default, built-in
/// constant as last resort). Re-read on every wand run — no caching.
/// </summary>
public class CommitMessagePromptService : UserPromptTemplateService, ICommitMessagePromptService
{
    /// <summary>Placeholder tokens the template must carry.</summary>
    private const string FileListToken = "{file_list}";
    private const string DiffToken = "{diff}";
    private const string ContextToken = "{context}";

    /// <summary>
    /// The built-in template (also the source of the shipped default file): a Conventional
    /// Commits generator with the three placeholders.
    /// </summary>
    private const string DefaultTemplate = """
        You are a git commit message generator. Given a diff (and optional context), output ONE commit message following Conventional Commits.

        FORMAT:
        <type>(<scope>): <subject>

        <body>

        <footer>

        RULES:
        - type: feat, fix, refactor, perf, test, docs, style, chore, build, ci
        - scope: optional, lowercase, the affected module/service/package (infer from changed file paths)
        - subject: imperative mood ("add", not "added"/"adds"), lowercase start, no trailing period, max 72 chars
        - body: optional, wrap at 100 chars, explain WHAT changed and WHY (not how) — omit if the subject is fully self-explanatory
        - footer: only if there are breaking changes ("BREAKING CHANGE: ...") or issue refs ("Closes #123") supplied in context
        - If the diff touches multiple unrelated concerns, pick the dominant one for the subject and note the rest briefly in the body
        - Never invent file names, ticket numbers, or reasoning not supported by the diff
        - Output ONLY the commit message text — no markdown fences, no explanation, no quotes around it

        INPUT:
        Changed files: {file_list}

        Diff:
        {diff}

        Additional context (ticket/issue, manual notes — may be empty):
        {context}
        """;

    public CommitMessagePromptService() : base("commit-message.md", "opencode/commit-message.md", DefaultTemplate)
    {
    }

    public string BuildPrompt(string fileList, string diff, string context)
    {
        return LoadTemplate()
            .Replace(FileListToken, fileList)
            .Replace(DiffToken, diff)
            .Replace(ContextToken, context);
    }
}
