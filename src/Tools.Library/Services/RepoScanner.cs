using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-system implementation of <see cref="IRepoScanner"/>. Walks configured folders
/// recursively up to a depth limit, honoring exclusions, to discover git repositories
/// (one <see cref="Repo"/> per .git folder's parent). Auto-tags repos whose path
/// contains the configured platform folder name.
/// </summary>
public class RepoScanner : IRepoScanner
{
    /// <summary>
    /// The auto-tag applied to repos whose folder path matches the configured
    /// <see cref="ReposSettings.PlatformFolderName"/>. Recomputed on every scan and
    /// therefore excluded from the user-defined tag merge in <see cref="RepoService"/>.
    /// Mirrors <see cref="Repo.PlatformTag"/>; kept for backward reference from the
    /// services layer.
    /// </summary>
    public const string PlatformTag = Repo.PlatformTag;

    public Task<RepoScanResult> ScanAsync(ReposSettings settings)
    {
        var repos = new List<Repo>();

        var scanFolders = settings.RepoScanFolders ?? Array.Empty<string>();
        var gitPattern = settings.GitFolderPattern ?? ".git";
        var platformPattern = settings.PlatformFolderName ?? PlatformTag;
        var slnPatterns = ParseSolutionPatterns(settings.SolutionFilePattern);
        var maxDepth = settings.MaxScanDepth > 0 ? settings.MaxScanDepth : 3;

        foreach (var folderPath in scanFolders)
        {
            if (!Directory.Exists(folderPath)) continue;

            foreach (var dir in GetAccessibleDirectoriesRecursively(folderPath, gitPattern, maxDepth, settings.ExcludedFolders))
            {
                var parentDir = Path.GetDirectoryName(dir);
                if (parentDir == null) continue;

                var solutionFiles = GetSolutionFiles(parentDir, slnPatterns);

                var repo = new Repo
                {
                    Name = Path.GetFileName(parentDir.TrimEnd(Path.DirectorySeparatorChar)),
                    FolderPath = parentDir,
                    SolutionPath = solutionFiles.Length > 0 ? solutionFiles[0] : null
                };

                // Auto-tag platform folders so the filter list stays meaningful without
                // requiring users to tag each one by hand.
                if (parentDir.Contains(platformPattern, StringComparison.OrdinalIgnoreCase))
                {
                    repo.AddTag(PlatformTag);
                }

                repos.Add(repo);
            }
        }

        var result = new RepoScanResult
        {
            // Distinct by FolderPath: a repo reachable through multiple scan roots or
            // nested layouts should appear only once.
            Repos = repos
                .GroupBy(r => r.FolderPath)
                .Select(g => g.First())
                .OrderBy(r => r.Name)
                .ToList()
        };
        return Task.FromResult(result);
    }

    /// <summary>
    /// Splits the solution file pattern setting into individual patterns. Supports
    /// comma- or semicolon-separated values (e.g. <c>"*.sln, *.slnx"</c>) so multiple
    /// solution formats can be discovered at once. Falls back to <c>"*.sln"</c>.
    /// </summary>
    private static string[] ParseSolutionPatterns(string? solutionFilePattern)
    {
        var patterns = (solutionFilePattern ?? string.Empty)
            .Split(new[] { ',', ';' }, StringSplitOptions.RemoveEmptyEntries)
            .Select(p => p.Trim())
            .Where(p => p.Length > 0)
            .ToArray();

        return patterns.Length == 0 ? new[] { "*.sln" } : patterns;
    }

    /// <summary>
    /// Returns the solution files in <paramref name="directory"/> matching any of the
    /// given <paramref name="patterns"/>, de-duplicated and ordered so the result is
    /// stable across scans.
    /// </summary>
    private static string[] GetSolutionFiles(string directory, string[] patterns)
    {
        var solutionFiles = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var pattern in patterns)
        {
            foreach (var file in Directory.GetFiles(directory, pattern))
            {
                solutionFiles.Add(file);
            }
        }

        return solutionFiles
            .OrderBy(f => f, StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private static IEnumerable<string> GetAccessibleDirectoriesRecursively(
        string rootPath,
        string searchPattern,
        int maxDepth,
        string[]? excludedFolders)
    {
        return GetAccessibleDirectoriesRecursively(rootPath, searchPattern, currentDepth: 1, maxDepth, excludedFolders);
    }

    private static IEnumerable<string> GetAccessibleDirectoriesRecursively(
        string rootPath,
        string searchPattern,
        int currentDepth,
        int maxDepth,
        string[]? excludedFolders)
    {
        var foundDirectories = new List<string>();
        try
        {
            foreach (var dir in Directory.EnumerateDirectories(rootPath, searchPattern, SearchOption.TopDirectoryOnly))
            {
                foundDirectories.Add(dir);
            }

            // Stop descending once we reach the configured depth limit.
            if (currentDepth >= maxDepth)
            {
                return foundDirectories;
            }

            excludedFolders ??= Array.Empty<string>();

            foreach (var subDir in Directory.EnumerateDirectories(rootPath))
            {
                var subDirName = Path.GetFileName(subDir);

                if (subDirName.Equals(searchPattern, StringComparison.OrdinalIgnoreCase) ||
                    excludedFolders.Any(ex => ex.Equals(subDir, StringComparison.OrdinalIgnoreCase)))
                {
                    continue;
                }

                foundDirectories.AddRange(GetAccessibleDirectoriesRecursively(subDir, searchPattern, currentDepth + 1, maxDepth, excludedFolders));
            }
        }
        catch (UnauthorizedAccessException)
        {
            Log.Logger.Warning("Access denied to folder: {Path}", rootPath);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error accessing folder {Path}", rootPath);
        }

        return foundDirectories;
    }
}
