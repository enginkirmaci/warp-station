using System.IO;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Loads the commit-message prompt template from the user's settings folder and fills
/// its placeholders. The user file (<c>%USERPROFILE%\.devtools\settings\commit-message.md</c>)
/// is seeded once from the shipped default (<c>&lt;install&gt;/settings/commit-message.md</c>)
/// so upgrades never clobber user edits; if both are missing the built-in default below is
/// used and written back, so the file a user can edit always materializes. Re-read on
/// every wand run — no caching — so template edits take effect immediately.
/// </summary>
public class CommitMessagePromptService : ICommitMessagePromptService
{
    /// <summary>The user-editable template, inside the settings folder.</summary>
    private static readonly string UserFilePath = UserPaths.GetUserDataFile("settings", "commit-message.md");

    /// <summary>Shipped default under the install directory's settings folder.</summary>
    private const string ShippedRelPath = "commit-message.md";

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

    public string BuildPrompt(string fileList, string diff, string context)
    {
        var template = LoadTemplate();
        return template
            .Replace(FileListToken, fileList)
            .Replace(DiffToken, diff)
            .Replace(ContextToken, context);
    }

    /// <summary>
    /// Resolves the template text: the user's file wins; the shipped default seeds it;
    /// the built-in constant is the last resort (and writes itself back so the editable
    /// file exists). Read errors fall back to the built-in constant.
    /// </summary>
    private static string LoadTemplate()
    {
        try
        {
            UserPaths.SeedFromDefault(UserFilePath, ShippedRelPath);
            if (File.Exists(UserFilePath))
            {
                return File.ReadAllText(UserFilePath);
            }

            Directory.CreateDirectory(Path.GetDirectoryName(UserFilePath)!);
            File.WriteAllText(UserFilePath, DefaultTemplate);
        }
        catch
        {
            // A broken template location must not kill the wand — use the built-in text.
        }

        return DefaultTemplate;
    }
}
