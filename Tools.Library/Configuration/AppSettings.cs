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

    /// <summary>
    /// Gets or sets the Focus Timer settings.
    /// </summary>
    public FocusTimerSettings? FocusTimer { get; set; }
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

    /// <summary>
    /// Gets or sets whether to clear NuGet cache after copying packages.
    /// </summary>
    public bool ClearCacheOnCopy { get; set; } = true;
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
    /// Gets or sets the terminal executable path or command.
    /// </summary>
    public string? TerminalExecutable { get; set; } = "wt";

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

/// <summary>
/// Settings for Focus Timer functionality.
/// </summary>
public class FocusTimerSettings
{
    /// <summary>
    /// Gets or sets the work start time (e.g., "09:00").
    /// </summary>
    public string WorkStartTime { get; set; } = "09:00";

    /// <summary>
    /// Gets or sets the work end time (e.g., "18:00").
    /// </summary>
    public string WorkEndTime { get; set; } = "18:00";

    /// <summary>
    /// Gets or sets the lunch start time (e.g., "13:00").
    /// </summary>
    public string LunchStartTime { get; set; } = "13:00";

    /// <summary>
    /// Gets or sets the lunch duration in minutes.
    /// </summary>
    public int LunchDurationMinutes { get; set; } = 60;

    /// <summary>
    /// Gets or sets the total daily break time in minutes.
    /// </summary>
    public int TotalDailyBreakMinutes { get; set; } = 60;

    /// <summary>
    /// Gets or sets the desired number of breaks per day.
    /// </summary>
    public int DesiredBreakCount { get; set; } = 4;

    /// <summary>
    /// Gets or sets the timer visibility mode.
    /// 0 = Always visible, 1 = Only on notification.
    /// </summary>
    public int TimerVisibilityMode { get; set; } = 0;

    /// <summary>
    /// Gets or sets the window corner position.
    /// 0 = BottomRight, 1 = BottomLeft, 2 = TopRight, 3 = TopLeft.
    /// </summary>
    public int WindowCornerPosition { get; set; } = 0;

    /// <summary>
    /// Gets or sets whether to play a sound on notification.
    /// </summary>
    public bool PlaySoundOnNotification { get; set; } = true;

    /// <summary>
    /// Gets or sets the persisted state for daily tracking.
    /// </summary>
    public FocusTimerPersistedState? PersistedState { get; set; }
}

/// <summary>
/// Persisted state for Focus Timer that resets daily.
/// </summary>
public class FocusTimerPersistedState
{
    /// <summary>
    /// Gets or sets the date this state was last updated.
    /// </summary>
    public string? LastResetDate { get; set; }

    /// <summary>
    /// Gets or sets the remaining break pool in minutes.
    /// </summary>
    public double CurrentBreakPoolMinutes { get; set; }

    /// <summary>
    /// Gets or sets the remaining break count for the day.
    /// </summary>
    public int RemainingBreakCount { get; set; }

    /// <summary>
    /// Gets or sets the total break time taken today in minutes.
    /// </summary>
    public double BreakTimeTakenMinutes { get; set; }
}