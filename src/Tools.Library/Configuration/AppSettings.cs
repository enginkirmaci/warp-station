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
    /// Gets or sets a value indicating whether the NuGet Local tool is surfaced in the
    /// GUI (the tools dropdown entry and the title-bar watch chip) and allowed to
    /// start watching. Defaults to <see langword="true"/> so settings files without
    /// the key keep the tool. Edited in the Repo Settings drawer.
    /// </summary>
    public bool EnableNuget { get; set; } = true;

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
    /// <summary>Canonical default for <see cref="GitFolderPattern"/>.</summary>
    public const string DefaultGitFolderPattern = "*.git";

    /// <summary>Canonical default for <see cref="SolutionFilePattern"/>.</summary>
    public const string DefaultSolutionFilePattern = "*.sln,*.slnx";

    /// <summary>Canonical default for <see cref="PlatformFolderName"/>.</summary>
    public const string DefaultPlatformFolderName = "platform";

    /// <summary>Canonical default for <see cref="VSCodeExecutable"/>.</summary>
    public const string DefaultVSCodeExecutable = "code";

    /// <summary>Canonical default for <see cref="TerminalExecutable"/>.</summary>
    public const string DefaultTerminalExecutable = "wt";

    /// <summary>Canonical default for <see cref="OpenCodeExecutable"/>.</summary>
    public const string DefaultOpenCodeExecutable = "opencode";

    /// <summary>Canonical default for <see cref="ZCodeExecutable"/>.</summary>
    public const string DefaultZCodeExecutable = "zcode";

    /// <summary>Canonical default for <see cref="GitHubExecutable"/>.</summary>
    public const string DefaultGitHubExecutable = "gh";

    /// <summary>Canonical default for <see cref="MaxScanDepth"/>.</summary>
    public const int DefaultMaxScanDepth = 3;

    /// <summary>Canonical default for <see cref="BranchNamePrefix"/>.</summary>
    public const string DefaultBranchNamePrefix = "users/enginkirmaci";

    /// <summary>
    /// Gets a fresh <see cref="ReposSettings"/> carrying the canonical defaults. The
    /// per-field constants above are the single source of truth — the per-property
    /// initializers and this instance both draw from them, so <c>new ReposSettings()</c>
    /// and <see cref="Defaults"/> always agree. Callers wanting a single field should read
    /// the constant directly (e.g. <c>ReposSettings.DefaultMaxScanDepth</c>) to avoid the
    /// allocation.
    /// </summary>
    public static ReposSettings Defaults => new()
    {
        GitFolderPattern = DefaultGitFolderPattern,
        SolutionFilePattern = DefaultSolutionFilePattern,
        PlatformFolderName = DefaultPlatformFolderName,
        VSCodeExecutable = DefaultVSCodeExecutable,
        TerminalExecutable = DefaultTerminalExecutable,
        OpenCodeExecutable = DefaultOpenCodeExecutable,
        ZCodeExecutable = DefaultZCodeExecutable,
        GitHubExecutable = DefaultGitHubExecutable,
        MaxScanDepth = DefaultMaxScanDepth,
        BranchNamePrefix = DefaultBranchNamePrefix
    };

    /// <summary>
    /// Gets or sets the folders to scan for repositories.
    /// </summary>
    public string[]? RepoScanFolders { get; set; }

    /// <summary>
    /// Gets or sets the git folder pattern to search for.
    /// </summary>
    public string? GitFolderPattern { get; set; } = DefaultGitFolderPattern;

    /// <summary>
    /// Gets or sets the solution file pattern(s) to search for. Multiple patterns may be
    /// comma- or semicolon-separated (e.g. <c>"*.sln,*.slnx"</c>) so both classic and
    /// XML-based solution formats are discovered. Defaults to <c>"*.sln,*.slnx"</c>.
    /// </summary>
    public string? SolutionFilePattern { get; set; } = DefaultSolutionFilePattern;

    /// <summary>
    /// Gets or sets the platform folder name identifier. A repo whose path contains
    /// this substring is auto-tagged <c>platform</c>.
    /// </summary>
    public string? PlatformFolderName { get; set; } = DefaultPlatformFolderName;

    /// <summary>
    /// Gets or sets the VS Code executable path or command.
    /// </summary>
    public string? VSCodeExecutable { get; set; } = DefaultVSCodeExecutable;

    /// <summary>
    /// Gets or sets the VS Code profile name to launch with (passed as
    /// <c>--profile &lt;name&gt;</c>). When empty, VS Code opens with the default profile.
    /// </summary>
    public string? VSCodeProfile { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the terminal executable path or command.
    /// </summary>
    public string? TerminalExecutable { get; set; } = DefaultTerminalExecutable;

    /// <summary>
    /// Gets or sets the IDE executable used to open solutions. On Windows the .sln shell
    /// association is used when this is empty; on other platforms a well-known .NET IDE
    /// (e.g. Rider) is auto-detected from PATH when empty.
    /// </summary>
    public string? IdeExecutable { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the OpenCode executable path or command.
    /// </summary>
    public string? OpenCodeExecutable { get; set; } = DefaultOpenCodeExecutable;

    /// <summary>
    /// Gets or sets the ZCode CLI executable path or command, launched in a repo folder
    /// via the configured terminal.
    /// </summary>
    public string? ZCodeExecutable { get; set; } = DefaultZCodeExecutable;

    /// <summary>
    /// Gets or sets a value indicating whether the terminal launch button shows on repo
    /// rows. Purely a visibility switch — the machine is not probed for the executable;
    /// a configured-but-missing one is reported at launch time. Defaults to
    /// <see langword="true"/>.
    /// </summary>
    public bool EnableTerminal { get; set; } = true;

    /// <summary>
    /// Gets or sets a value indicating whether the VS Code launch button shows on repo
    /// rows. Purely a visibility switch — the machine is not probed for the executable.
    /// Defaults to <see langword="true"/>.
    /// </summary>
    public bool EnableVSCode { get; set; } = true;

    /// <summary>
    /// Gets or sets a value indicating whether the Visual Studio (open solution) button
    /// shows on repo rows. Visual Studio only exists on Windows, so the button hides on
    /// other platforms regardless of this flag. No installation probe runs. Defaults to
    /// <see langword="true"/>.
    /// </summary>
    public bool EnableVisualStudio { get; set; } = true;

    /// <summary>
    /// Gets or sets a value indicating whether the zcode launch button shows on repo
    /// rows. Purely a visibility switch — the machine is not probed for the executable.
    /// Defaults to <see langword="true"/>.
    /// </summary>
    public bool EnableZCode { get; set; } = true;

    /// <summary>
    /// Gets or sets a value indicating whether the GitHub integration is enabled. When
    /// <see langword="false"/> the GitHub column/tab is hidden <em>and</em> the <c>gh</c>
    /// CLI is never queried, so disabling it costs nothing at runtime. Defaults to
    /// <see langword="true"/>.
    /// </summary>
    public bool EnableGitHub { get; set; } = true;

    /// <summary>
    /// Gets or sets the GitHub CLI (<c>gh</c>) executable path or command used to query
    /// open pull requests and issues for the GitHub column.
    /// </summary>
    public string? GitHubExecutable { get; set; } = DefaultGitHubExecutable;

    /// <summary>
    /// Gets or sets a value indicating whether the Azure DevOps integration is enabled.
    /// When <see langword="false"/> the column/tab is hidden <em>and</em> the Azure
    /// DevOps REST API is never called, so disabling it costs nothing at runtime.
    /// Defaults to <see langword="true"/> (a column without a configured token stays
    /// empty rather than probing).
    /// </summary>
    public bool EnableAzureDevOps { get; set; } = true;

    /// <summary>
    /// Gets or sets the Azure DevOps personal access token (PAT) used to query open pull
    /// requests, work items and pipeline runs for the Azure DevOps column. The token only
    /// needs <c>Build (read)</c>, <c>Code (read)</c> and <c>Work Items (read)</c> scopes.
    /// When empty, the <c>AZURE_DEVOPS_PAT</c> (then <c>AZURE_DEVOPS_EXT_PAT</c>)
    /// environment variables are consulted instead.
    /// </summary>
    public string? AzureDevOpsPat { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the base URL of a company-hosted Azure DevOps Server, e.g.
    /// <c>https://devops.company.com</c> or <c>https://tfs.company.com/tfs</c>. Git
    /// remotes under this host are recognized in addition to the public hosts
    /// (<c>dev.azure.com</c>, <c>*.visualstudio.com</c>) and the REST API is called
    /// against it. When empty, only the public hosts are matched.
    /// </summary>
    public string? AzureDevOpsUrl { get; set; } = string.Empty;

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
    /// Gets or sets the maximum folder depth used only by the Add Repositories
    /// dialog's scan: how many folder levels below the picked folder are searched.
    /// A value of 1 scans only the picked folder's direct children, 2 includes their
    /// subfolders, and so on. The main Repos page listing always scans non-recursively
    /// and ignores this value. Defaults to 3.
    /// </summary>
    public int MaxScanDepth { get; set; } = DefaultMaxScanDepth;

    /// <summary>
    /// Gets or sets the branch name prefix pre-typed into the new-branch drawer's name
    /// field (the drawer appends the path separator, so the user's first keystroke
    /// continues after it, e.g. <c>users/enginkirmaci/</c>). When empty the field starts
    /// empty. Defaults to <see cref="DefaultBranchNamePrefix"/>.
    /// </summary>
    public string? BranchNamePrefix { get; set; } = DefaultBranchNamePrefix;
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
    public bool EnableOpenCode { get; set; }

    /// <summary>
    /// Gets or sets the model id (<c>provider/model-id</c>, as printed by
    /// <c>opencode models</c>) the Repos page's quick-launch button passes to opencode
    /// verbatim — the button never queries the CLI for the model catalog and shows an
    /// error alert when this is empty. The OpenCode drawer's per-launch picker
    /// preselects it too. Edited in the Repo Settings drawer or settings.json.
    /// </summary>
    public string? DefaultModel { get; set; } = string.Empty;

    /// <summary>
    /// Gets or sets the model id used ONLY by the Changes tab's commit-message wand
    /// (<c>opencode run</c> one-shot), independent of <see cref="DefaultModel"/> — a
    /// cheap/fast model can be dedicated to commit messages while interactive sessions
    /// launch with the default. When empty, the wand falls back to
    /// <see cref="DefaultModel"/>. Edited in the Repo Settings drawer or settings.json.
    /// </summary>
    public string? CommitModel { get; set; }
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
    /// Gets or sets a value indicating whether the Clipboard Password tool is enabled
    /// (surfaced in the tools dropdown). When <see langword="false"/>, the stored
    /// password can still be pasted via the Ctrl+Shift+V hotkey; only the GUI entry
    /// points are concealed. Defaults to <see langword="false"/> (hidden). Configured
    /// manually via settings.json. (Generalized from the legacy INVERTED
    /// <c>HideFromGui</c> key; the loader migrates it.)
    /// </summary>
    public bool EnableClipboardPassword { get; set; }
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
    /// when the user signs in. Configured via settings.json (no GUI surface); both the
    /// DevTools supervisor and the Tools GUI reconcile the OS registration to this flag
    /// on every launch (Windows: per-user registry Run key; other platforms: XDG
    /// autostart entry, each targeting <c>AutoStartHelper.ResolveBootTarget()</c> —
    /// the DevTools supervisor when it sits next to the app, otherwise the app itself).
    /// </summary>
    public bool StartAtBoot { get; set; }
}