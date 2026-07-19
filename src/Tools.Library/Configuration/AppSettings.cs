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
    /// Gets or sets the opencode serve settings (the headless <c>opencode serve</c>
    /// subprocess the app manages for live status and auto-approve).
    /// </summary>
    public OpenCodeServeSettings? OpenCode { get; set; }

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
    /// Gets or sets the terminal executable path or command.
    /// </summary>
    public string? TerminalExecutable { get; set; } = "wt";

    /// <summary>
    /// Gets or sets the OpenCode executable path or command.
    /// </summary>
    public string? OpenCodeExecutable { get; set; } = "opencode";

    /// <summary>
    /// Gets or sets the folders to exclude during scanning.
    /// </summary>
    public string[]? ExcludedFolders { get; set; }

    /// <summary>
    /// Gets or sets the maximum folder depth to scan recursively.
    /// A value of 1 scans only the root scan folder, 2 includes its immediate
    /// subfolders, and so on. Defaults to 3.
    /// </summary>
    public int MaxScanDepth { get; set; } = 3;
}

/// <summary>
/// Settings for the managed <c>opencode serve</c> subprocess. The app starts a single
/// headless server (<c>opencode serve --hostname &lt;Host&gt; --port &lt;Port&gt;</c>) and
/// connects over HTTP for live session status and runtime tool-approval (auto-approve).
/// </summary>
public class OpenCodeServeSettings
{
    /// <summary>
    /// Gets or sets a value indicating whether the OpenCode integration (serve
    /// indicator, per-repo serve status, and the OpenCode launch panel with
    /// auto-approve) is surfaced in the GUI. When <see langword="false"/>, the
    /// <c>opencode serve</c> subprocess is not started and all OpenCode UI is
    /// hidden. Defaults to <see langword="false"/> (hidden). Configured manually
    /// via settings.json.
    /// </summary>
    public bool Enabled { get; set; }

    /// <summary>
    /// Gets or sets a value indicating whether the serve subprocess is started and
    /// connected automatically when the Repos page opens.
    /// </summary>
    public bool AutoConnect { get; set; } = true;

    /// <summary>
    /// Gets or sets the hostname passed to <c>opencode serve --hostname</c>. Defaults to
    /// <c>127.0.0.1</c> (loopback only).
    /// </summary>
    public string Host { get; set; } = "127.0.0.1";

    /// <summary>
    /// Gets or sets the port passed to <c>opencode serve --port</c>. Defaults to
    /// <c>4096</c> (opencode's documented default). A fixed port lets the app connect
    /// without parsing the server's startup output.
    /// </summary>
    public int Port { get; set; } = 4096;

    /// <summary>
    /// Gets or sets the optional bearer/basic-auth password. When set, it is provided to
    /// the subprocess via the <c>OPENCODE_SERVER_PASSWORD</c> environment variable and sent
    /// to the server as HTTP basic auth (username <c>opencode</c>). When unset the server is
    /// unsecured (acceptable on loopback).
    /// </summary>
    public string? AuthToken { get; set; }

    /// <summary>
    /// Gets or sets the default auto-approve state applied to newly launched opencode
    /// instances. The per-launch toggle in the OpenCode panel overrides this.
    /// </summary>
    public bool AutoApprove { get; set; }

    /// <summary>
    /// Gets or sets the opencode executable used to run <c>serve</c>. Falls back to
    /// <c>opencode</c>. This is the same binary used by the terminal launch path
    /// (<see cref="ReposSettings.OpenCodeExecutable"/>); kept separate so the two features
    /// can point at different builds if needed.
    /// </summary>
    public string? OpenCodeExecutable { get; set; } = "opencode";
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
}