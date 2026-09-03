namespace Tools.Library.Configuration;

/// <summary>
/// Root object mirroring the settings.json structure.
/// </summary>
public class AppSettings
{
    /// <summary>
    /// Gets or sets the Nuget Local settings.
    /// </summary>
    public NugetLocalSettings? NugetLocal { get; set; }

    /// <summary>
    /// Gets or sets the Repos settings.
    /// </summary>
    public ReposSettings? Repos { get; set; }

    /// <summary>
    /// Gets or sets the Clipboard Password settings.
    /// </summary>
    public ClipboardPasswordSettings? ClipboardPassword { get; set; }

    /// <summary>
    /// Gets or sets the SnapIt settings.
    /// </summary>
    public SnapItSettings? SnapIt { get; set; }

    /// <summary>
    /// Gets or sets the OpenCode integration settings (the launch panel on the Repos page).
    /// </summary>
    public OpenCodeSettings? OpenCode { get; set; }

    /// <summary>
    /// Gets or sets the general application settings.
    /// </summary>
    public GeneralSettings? General { get; set; }
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
/// Settings specific to Repos functionality.
/// </summary>
public class ReposSettings
{
    /// <summary>
    /// Gets or sets the folders to scan for repositories.
    /// </summary>
    public string[]? RepoScanFolders { get; set; }

    /// <summary>
    /// Gets or sets the git folder pattern to search for.
    /// </summary>
    public string? GitFolderPattern { get; set; } = "*.git";

    /// <summary>
    /// Gets or sets the solution file pattern(s) to search for. Multiple patterns may be
    /// comma- or semicolon-separated (e.g. <c>"*.sln,*.slnx"</c>) so both classic and
    /// XML-based solution formats are discovered. Defaults to <c>"*.sln,*.slnx"</c>.
    /// </summary>
    public string? SolutionFilePattern { get; set; } = "*.sln,*.slnx";

    /// <summary>
    /// Gets or sets the platform folder name identifier. A repo whose path contains
    /// this substring is auto-tagged <c>platform</c>.
    /// </summary>
    public string? PlatformFolderName { get; set; } = "platform";

    /// <summary>
    /// Gets or sets the VS Code executable path or command.
    /// </summary>
    public string? VSCodeExecutable { get; set; } = "code";

    /// <summary>
    /// Gets or sets the VS Code profile name to launch with (passed as
    /// <c>--profile &lt;name&gt;</c>). When empty, VS Code opens with the default profile.
    /// </summary>
    public string? VSCodeProfile { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the terminal executable path or command.
    /// </summary>
    public string? TerminalExecutable { get; set; } = "wt";

    /// <summary>
    /// Gets or sets the IDE executable used to open solutions. On Windows the .sln shell
    /// association is used when this is empty; on other platforms a well-known .NET IDE
    /// (e.g. Rider) is auto-detected from PATH when empty.
    /// </summary>
    public string? IdeExecutable { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the OpenCode executable path or command.
    /// </summary>
    public string? OpenCodeExecutable { get; set; } = "opencode";

    /// <summary>
    /// Gets or sets the ZCode CLI executable path or command, launched in a repo folder
    /// via the configured terminal.
    /// </summary>
    public string? ZCodeExecutable { get; set; } = "zcode";

    /// <summary>
    /// Gets or sets a value indicating whether the GitHub column is shown on the Repos
    /// page. When <see langword="false"/> the column is hidden <em>and</em> the <c>gh</c>
    /// CLI is never queried, so disabling it costs nothing at runtime. Defaults to
    /// <see langword="true"/>.
    /// </summary>
    public bool ShowGitHubColumn { get; set; } = true;

    /// <summary>
    /// Gets or sets the GitHub CLI (<c>gh</c>) executable path or command used to query
    /// open pull requests and issues for the GitHub column.
    /// </summary>
    public string? GitHubExecutable { get; set; } = "gh";

    /// <summary>
    /// Gets or sets a value indicating whether the Azure DevOps column is shown on the
    /// Repos page. When <see langword="false"/> the column is hidden <em>and</em> the
    /// Azure DevOps REST API is never called, so disabling it costs nothing at runtime.
    /// Defaults to <see langword="true"/> (a column without a configured token stays
    /// empty rather than probing).
    /// </summary>
    public bool ShowAzureDevOpsColumn { get; set; } = true;

    /// <summary>
    /// Gets or sets the Azure DevOps personal access token (PAT) used to query open pull
    /// requests, work items and pipeline runs for the Azure DevOps column. The token only
    /// needs <c>Build (read)</c>, <c>Code (read)</c> and <c>Work Items (read)</c> scopes.
    /// When empty, the <c>AZURE_DEVOPS_PAT</c> (then <c>AZURE_DEVOPS_EXT_PAT</c>)
    /// environment variables are consulted instead.
    /// </summary>
    public string? AzureDevOpsPat { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the folders to exclude during scanning.
    /// </summary>
    public string[]? ExcludedFolders { get; set; }

    /// <summary>
    /// Gets or sets the Repos page list sort order. Favorites always float to the top in
    /// every mode; this orders the rest. Persisted so the choice survives restarts.
    /// Defaults to <see cref="RepoSortMode.Name"/> (the historical ordering).
    /// </summary>
    public RepoSortMode SortMode { get; set; } = RepoSortMode.Name;

    /// <summary>
    /// Gets or sets the maximum folder depth to scan recursively.
    /// A value of 1 scans only the root scan folder, 2 includes its immediate
    /// subfolders, and so on. Defaults to 3.
    /// </summary>
    public int MaxScanDepth { get; set; } = 3;
}

/// <summary>
/// Settings for the OpenCode integration (the launch panel on the Repos page). Models are
/// listed by running <c>opencode models</c> as a one-shot process; no server is managed.
/// </summary>
public class OpenCodeSettings
{
    /// <summary>
    /// Gets or sets a value indicating whether the OpenCode integration (the per-repo
    /// OpenCode launch panel) is surfaced in the GUI. When <see langword="false"/>, all
    /// OpenCode UI is hidden. Defaults to <see langword="false"/> (hidden). Configured
    /// manually via settings.json.
    /// </summary>
    public bool Enabled { get; set; }

    /// <summary>
    /// Gets or sets the model id (<c>provider/model-id</c>, as printed by
    /// <c>opencode models</c>) preselected when the OpenCode panel opens and launched by
    /// Quick Open. The model catalog keeps this entry present (top of the list) even when
    /// the CLI does not print it, so the preselection always resolves. When empty, the
    /// first model from the CLI list is used, as before. Configured manually via
    /// settings.json.
    /// </summary>
    public string? DefaultModel { get; set; } = string.Empty;
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

    /// <summary>
    /// Gets or sets a value indicating whether the Clipboard Password page should be
    /// hidden from the dashboard and sidebar. When hidden, the stored password can
    /// still be pasted via the Ctrl+Shift+V hotkey; only the GUI entry points are
    /// concealed. Configured manually via settings.json.
    /// </summary>
    public bool HideFromGui { get; set; } = true;
}

/// <summary>
/// Application-wide general settings.
/// </summary>
public class GeneralSettings
{
    /// <summary>
    /// Gets or sets a value indicating whether the app should start with the
    /// main window minimized to the taskbar.
    /// </summary>
    public bool StartMinimized { get; set; }

    /// <summary>
    /// Gets or sets a value indicating whether the app should launch automatically
    /// when the user signs in (synced to the Windows registry Run key on startup).
    /// </summary>
    public bool StartAtBoot { get; set; }

    /// <summary>
    /// Gets or sets a value indicating whether the navigation sidebar should start
    /// collapsed to an icon-only rail. Persisted on every toggle so the user's
    /// last choice is restored on the next launch.
    /// </summary>
    public bool SidebarCollapsed { get; set; }
}