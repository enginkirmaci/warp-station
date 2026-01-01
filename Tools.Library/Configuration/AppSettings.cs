namespace Tools.Library.Configuration;

/// <summary>
/// Root object mirroring the settings.json structure.
/// </summary>
public class AppSettings
{
    /// <summary>
    /// Gets or sets the EF Tools page settings.
    /// </summary>
    public EFToolsPageSettings? EFToolsPage { get; set; }

    /// <summary>
    /// Gets or sets the Nuget Local settings.
    /// </summary>
    public NugetLocalSettings? NugetLocal { get; set; }

    /// <summary>
    /// Gets or sets the Workspaces settings.
    /// </summary>
    public WorkspacesSettings? Workspaces { get; set; }

    /// <summary>
    /// Gets or sets the Clipboard Password settings.
    /// </summary>
    public ClipboardPasswordSettings? ClipboardPassword { get; set; }
}

/// <summary>
/// Settings specific to EF Tools page.
/// </summary>
public class EFToolsPageSettings
{
    /// <summary>
    /// Gets or sets the repository code template.
    /// </summary>
    public string? RepositoryTemplate { get; set; }
}

/// <summary>
/// Settings specific to Nuget Local functionality.
/// </summary>
public class NugetLocalSettings
{
    /// <summary>
    /// Gets or sets the folder path to watch for new nuget packages.
    /// </summary>
    public string? WatchFolder { get; set; }

    /// <summary>
    /// Gets or sets the destination folder for copying packages.
    /// </summary>
    public string? CopyFolder { get; set; }

    /// <summary>
    /// Gets or sets the file filter pattern for nuget packages.
    /// </summary>
    public string? NugetPackageFilter { get; set; } = "*.nupkg";

    /// <summary>
    /// Gets or sets the delay in milliseconds before copying files.
    /// </summary>
    public int FileCopyDelayMs { get; set; } = 2000;

    /// <summary>
    /// Gets or sets the interval in seconds for resetting the counter.
    /// </summary>
    public int CountResetIntervalSeconds { get; set; } = 60;
}

/// <summary>
/// Settings specific to Workspaces functionality.
/// </summary>
public class WorkspacesSettings
{
    /// <summary>
    /// Gets or sets the folders to scan for workspaces.
    /// </summary>
    public string[]? WorkspaceScanFolders { get; set; }

    /// <summary>
    /// Gets or sets the git folder pattern to search for.
    /// </summary>
    public string? GitFolderPattern { get; set; } = "*.git";

    /// <summary>
    /// Gets or sets the solution file pattern to search for.
    /// </summary>
    public string? SolutionFilePattern { get; set; } = "*.sln";

    /// <summary>
    /// Gets or sets the platform folder name identifier.
    /// </summary>
    public string? PlatformFolderName { get; set; } = "platform";

    /// <summary>
    /// Gets or sets the VS Code executable path or command.
    /// </summary>
    public string? VSCodeExecutable { get; set; } = "code";

    /// <summary>
    /// Gets or sets the folders to exclude during scanning.
    /// </summary>
    public string[]? ExcludedFolders { get; set; }
}

/// <summary>
/// Settings for Clipboard Password functionality.
/// </summary>
public class ClipboardPasswordSettings
{
    /// <summary>
    /// Gets or sets the encrypted password.
    /// </summary>
    public string? EncryptedPassword { get; set; }
}
