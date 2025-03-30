namespace Tools.Library.Entities;

// Root object mirroring the settings.json structure
public class AppSettings
{
    public EFToolsPageSettings? EFToolsPage { get; set; }
    public NugetLocalSettings? NugetLocal { get; set; }
    public WorkspacesSettings? Workspaces { get; set; }
}

// Settings specific to EFToolsPageViewModel
public class EFToolsPageSettings
{
    public string? RepositoryTemplate { get; set; }
}

// Settings specific to NugetLocalViewModel
public class NugetLocalSettings
{
    public string? WatchFolder { get; set; }
    public string? CopyFolder { get; set; }
    public string? NugetPackageFilter { get; set; } = "*.nupkg";
    public int FileCopyDelayMs { get; set; } = 2000;
    public int CountResetIntervalSeconds { get; set; } = 60;
}

// Settings specific to WorkspacesViewModel
public class WorkspacesSettings
{
    public string[]? WorkspaceScanFolders { get; set; }
    public string? GitFolderPattern { get; set; } = "*.git";
    public string? SolutionFilePattern { get; set; } = "*.sln";
    public string? PlatformFolderName { get; set; } = "platform";
    public string? VSCodeExecutable { get; set; } = "code";
    public string[]? ExcludedFolders { get; set; } // Added for excluding folders during scan
}
