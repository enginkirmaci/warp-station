using System.IO;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Loads the README prompt template from the user's opencode folder and fills its
/// placeholders (<c>~/.devtools/opencode/readme.md</c>, seeded once from the shipped
/// default when packaged with one; the built-in constant below materializes the
/// editable file otherwise). Re-read on every generation — see
/// <see cref="UserPromptTemplateService"/>.
/// </summary>
public class ReadmePromptService : UserPromptTemplateService, IReadmePromptService
{
    /// <summary>Placeholder tokens the template must carry.</summary>
    private const string RepoNameToken = "{repo_name}";
    private const string FileTreeToken = "{file_tree}";
    private const string ContextToken = "{context}";

    /// <summary>
    /// The built-in template (also the intended shipped default): a README.md writer
    /// driven by the project name and file tree.
    /// </summary>
    private const string DefaultTemplate = """
        You are a README.md generator. Given a repository's name and file tree (plus optional context), write a concise, professional README.md for the project.

        RULES:
        - Start with a single "# <project name>" heading and a one-paragraph description
        - Infer the project's purpose from the file tree; never invent features, commands, badges, or links the tree does not support
        - Include "## Installation" and "## Usage" sections with fenced code blocks kept generic
        - Write clear, simple English; keep the whole document under ~120 lines
        - Output ONLY the README markdown — no wrapping code fences, no explanation

        INPUT:
        Project name: {repo_name}

        File tree:
        {file_tree}

        Additional context (may be empty):
        {context}
        """;

    public ReadmePromptService() : base("readme.md", "opencode/readme.md", DefaultTemplate)
    {
    }

    public string BuildPrompt(string repoName, string fileTree, string context)
    {
        return LoadTemplate()
            .Replace(RepoNameToken, repoName)
            .Replace(FileTreeToken, fileTree)
            .Replace(ContextToken, context);
    }
}
