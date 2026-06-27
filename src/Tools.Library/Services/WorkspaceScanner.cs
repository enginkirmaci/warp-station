using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-system implementation of <see cref="IWorkspaceScanner"/>. Walks configured
/// folders recursively up to a depth limit, honoring exclusions, to discover solution
/// files (workspaces) and platform folders.
/// </summary>
public class WorkspaceScanner : IWorkspaceScanner
{
    public Task<WorkspaceScanResult> ScanAsync(WorkspacesSettings settings)
    {
        var workspaces = new List<WorkspaceItem>();
        var platforms = new List<WorkspaceItem>();

        var scanFolders = settings.WorkspaceScanFolders ?? Array.Empty<string>();
        var gitPattern = settings.GitFolderPattern ?? ".git";
        var platformPattern = settings.PlatformFolderName ?? "platform";
        var slnPattern = settings.SolutionFilePattern ?? "*.sln";
        var maxDepth = settings.MaxScanDepth > 0 ? settings.MaxScanDepth : 3;

        foreach (var folderPath in scanFolders)
        {
            if (!Directory.Exists(folderPath)) continue;

            foreach (var dir in GetAccessibleDirectoriesRecursively(folderPath, gitPattern, maxDepth, settings.ExcludedFolders))
            {
                var parentDir = Path.GetDirectoryName(dir);
                if (parentDir == null) continue;

                var solutionFiles = Directory.GetFiles(parentDir, slnPattern);
                foreach (var solutionFile in solutionFiles)
                {
                    workspaces.Add(new WorkspaceItem
                    {
                        SolutionName = Path.GetFileNameWithoutExtension(solutionFile),
                        FolderPath = Path.GetDirectoryName(solutionFile),
                        SolutionPath = solutionFile
                    });
                }

                if (dir.Contains(platformPattern, StringComparison.OrdinalIgnoreCase))
                {
                    platforms.Add(new WorkspaceItem
                    {
                        PlatformName = Path.GetFileName(parentDir.TrimEnd(Path.DirectorySeparatorChar)),
                        FolderPath = parentDir
                    });
                }
            }
        }

        var result = new WorkspaceScanResult
        {
            Workspaces = workspaces.DistinctBy(w => w.SolutionPath).OrderBy(w => w.SolutionName).ToList(),
            Platforms = platforms.DistinctBy(p => p.FolderPath).OrderBy(p => p.PlatformName).ToList()
        };
        return Task.FromResult(result);
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
