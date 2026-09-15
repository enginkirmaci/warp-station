using System.IO;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// The README pane's wand: assembles a project-context prompt (repo name + capped
/// two-level file tree) and runs it through opencode via
/// <see cref="OpenCodeWandGenerator"/>. Snapshot rule shared with the commit wand:
/// everything the prompt needs is captured BEFORE the run — a repo switch mid-run
/// must not leak into the answer.
/// </summary>
public sealed class ReadmeGenerator : OpenCodeWandGenerator
{
    private readonly IReadmePromptService _promptService;

    /// <summary>Upper bound on the file tree fed to the model.</summary>
    private const int MaxFileTreeLines = 200;

    /// <summary>Answer cap for generated markdown — long, but not a runaway.</summary>
    private const int MaxReadmeLength = 20000;

    /// <summary>Directories that carry no signal for a README.</summary>
    private static readonly string[] SkippedDirectories = [".git", "bin", "obj", "node_modules"];

    public ReadmeGenerator(
        IOpenCodeRunService openCodeRunService,
        IReadmePromptService promptService)
        : base(openCodeRunService)
        => _promptService = promptService;

    /// <summary>
    /// Runs the generation for the snapshotted repo; the cleaned markdown, or null
    /// when the folder is unreadable / the CLI fails / nothing usable came back.
    /// </summary>
    public async Task<string?> TryGenerateAsync(
        CancellationToken cancellationToken,
        Repo repo,
        string executable,
        string? model)
    {
        var tree = BuildFileTree(repo.FolderPath);
        if (tree is null) return null;

        var prompt = _promptService.BuildPrompt(repo.Name, tree, context: string.Empty);
        return await RunAsync(cancellationToken, executable, model, prompt, MaxReadmeLength);
    }

    /// <summary>
    /// Two-level listing (directories first, then root files) with build-output and
    /// VCS folders skipped; null when the folder is missing or unreadable.
    /// </summary>
    private static string? BuildFileTree(string? folder)
    {
        if (string.IsNullOrWhiteSpace(folder) || !Directory.Exists(folder))
            return null;

        try
        {
            var lines = new List<string>();
            foreach (var dir in Directory.EnumerateDirectories(folder).Order(StringComparer.OrdinalIgnoreCase))
            {
                if (lines.Count >= MaxFileTreeLines) break;
                var name = Path.GetFileName(dir);
                if (SkippedDirectories.Contains(name)) continue;

                lines.Add(name + "/");
                foreach (var file in Directory.EnumerateFiles(dir)
                             .Order(StringComparer.OrdinalIgnoreCase)
                             .Take(MaxFileTreeLines - lines.Count))
                {
                    lines.Add("  " + Path.GetFileName(file));
                }
            }

            foreach (var file in Directory.EnumerateFiles(folder)
                         .Order(StringComparer.OrdinalIgnoreCase)
                         .Take(MaxFileTreeLines - lines.Count))
            {
                lines.Add(Path.GetFileName(file));
            }

            return lines.Count == 0 ? null : string.Join('\n', lines);
        }
        catch
        {
            return null;
        }
    }
}
