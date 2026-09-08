using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;
using Tools.Services;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the <see cref="Views.Components.ReposSettingsComponent"/> (the former
/// Repo Settings modal dialog, now hosted in the floating tool drawer). Holds the
/// editing state for <see cref="ReposSettings"/> (multi-line text for the array
/// fields, plain strings for the rest) and translates between the two on load/save.
/// </summary>
public partial class ReposSettingsViewModel :
    DrawerDialogViewModelBase<ReposSettingsDrawerContext, ReposSettings>
{
    // Canonical defaults live on ReposSettings itself (the per-field Default* constants
    // that also back ReposSettings.Defaults); no private duplicates are kept here.

    [ObservableProperty]
    private string _repoScanFoldersText = string.Empty;

    [ObservableProperty]
    private string _excludedFoldersText = string.Empty;

    [ObservableProperty]
    private string _gitFolderPattern = ReposSettings.DefaultGitFolderPattern;

    [ObservableProperty]
    private string _solutionFilePattern = ReposSettings.DefaultSolutionFilePattern;

    [ObservableProperty]
    private string _platformFolderName = ReposSettings.DefaultPlatformFolderName;

    [ObservableProperty]
    private string _vsCodeExecutable = ReposSettings.DefaultVSCodeExecutable;

    [ObservableProperty]
    private string _vsCodeProfile = string.Empty;

    [ObservableProperty]
    private string _terminalExecutable = ReposSettings.DefaultTerminalExecutable;

    [ObservableProperty]
    private string _ideExecutable = string.Empty;

    [ObservableProperty]
    private string _openCodeExecutable = ReposSettings.DefaultOpenCodeExecutable;

    [ObservableProperty]
    private string _zCodeExecutable = ReposSettings.DefaultZCodeExecutable;

    /// <summary>Whether the terminal launch button shows (no installation probing).</summary>
    [ObservableProperty]
    private bool _enableTerminal = true;

    /// <summary>Whether the VS Code launch button shows (no installation probing).</summary>
    [ObservableProperty]
    private bool _enableVSCode = true;

    /// <summary>Whether the Visual Studio (open solution) button shows — Windows only,
    /// enforced by the page, so the checkbox hides nothing extra on other platforms.</summary>
    [ObservableProperty]
    private bool _enableVisualStudio = true;

    /// <summary>Whether the zcode launch button shows (no installation probing).</summary>
    [ObservableProperty]
    private bool _enableZCode = true;

    [ObservableProperty]
    private bool _enableGitHub = true;

    [ObservableProperty]
    private string _gitHubExecutable = ReposSettings.DefaultGitHubExecutable;

    [ObservableProperty]
    private bool _enableAzureDevOps = true;

    [ObservableProperty]
    private string _azureDevOpsPat = string.Empty;

    [ObservableProperty]
    private string _azureDevOpsUrl = string.Empty;

    [ObservableProperty]
    private string _maxScanDepth = ReposSettings.DefaultMaxScanDepth.ToString();

    /// <summary>
    /// Initializes a new instance. Editing state is seeded per open through
    /// <see cref="OnDrawerContextAsync"/> (the component is resolved fresh from DI each time).
    /// </summary>
    public ReposSettingsViewModel(IToolDrawerService toolDrawer)
        : base(toolDrawer)
    {
    }

    /// <summary>The open context's completion source the Save command resolves.</summary>
    protected override TaskCompletionSource<ReposSettings?> GetCompletion(ReposSettingsDrawerContext context)
        => context.Completion;

    /// <inheritdoc/>
    protected override void OnDrawerContext(ReposSettingsDrawerContext context)
        => LoadFrom(context.Current ?? new ReposSettings());

    /// <summary>
    /// Save: resolves the drawer context with the edited settings and closes the drawer
    /// (shared confirm plumbing on the base).
    /// </summary>
    [RelayCommand]
    private void Save() => ConfirmWith(BuildSettings());

    /// <summary>
    /// Builds a <see cref="ReposSettings"/> from the edited values, applying
    /// defaults for blank fields and parsing the multi-line text fields back to arrays.
    /// </summary>
    /// <returns>The edited repo settings.</returns>
    public ReposSettings BuildSettings()
    {
        return new ReposSettings
        {
            RepoScanFolders = ToLines(RepoScanFoldersText),
            ExcludedFolders = ToLines(ExcludedFoldersText),
            GitFolderPattern = WithDefault(GitFolderPattern, ReposSettings.DefaultGitFolderPattern),
            SolutionFilePattern = WithDefault(SolutionFilePattern, ReposSettings.DefaultSolutionFilePattern),
            PlatformFolderName = WithDefault(PlatformFolderName, ReposSettings.DefaultPlatformFolderName),
            VSCodeExecutable = WithDefault(VsCodeExecutable, ReposSettings.DefaultVSCodeExecutable),
            // No default: empty means "open VS Code with the default profile".
            VSCodeProfile = VsCodeProfile?.Trim() ?? string.Empty,
            TerminalExecutable = WithDefault(TerminalExecutable, ReposSettings.DefaultTerminalExecutable),
            // No default: empty means "auto-detect the IDE" (or use the .sln association on Windows).
            IdeExecutable = IdeExecutable?.Trim() ?? string.Empty,
            OpenCodeExecutable = WithDefault(OpenCodeExecutable, ReposSettings.DefaultOpenCodeExecutable),
            ZCodeExecutable = WithDefault(ZCodeExecutable, ReposSettings.DefaultZCodeExecutable),
            EnableTerminal = EnableTerminal,
            EnableVSCode = EnableVSCode,
            EnableVisualStudio = EnableVisualStudio,
            EnableZCode = EnableZCode,
            EnableGitHub = EnableGitHub,
            GitHubExecutable = WithDefault(GitHubExecutable, ReposSettings.DefaultGitHubExecutable),
            EnableAzureDevOps = EnableAzureDevOps,
            AzureDevOpsPat = AzureDevOpsPat?.Trim() ?? string.Empty,
            AzureDevOpsUrl = AzureDevOpsUrl?.Trim() ?? string.Empty,
            MaxScanDepth = int.TryParse(MaxScanDepth, out var depth) && depth > 0
                ? depth
                : ReposSettings.DefaultMaxScanDepth
        };
    }

    private void LoadFrom(ReposSettings settings)
    {
        RepoScanFoldersText = settings.RepoScanFolders is { Length: > 0 }
            ? string.Join(Environment.NewLine, settings.RepoScanFolders)
            : string.Empty;

        ExcludedFoldersText = settings.ExcludedFolders is { Length: > 0 }
            ? string.Join(Environment.NewLine, settings.ExcludedFolders)
            : string.Empty;

        GitFolderPattern = settings.GitFolderPattern ?? ReposSettings.DefaultGitFolderPattern;
        SolutionFilePattern = settings.SolutionFilePattern ?? ReposSettings.DefaultSolutionFilePattern;
        PlatformFolderName = settings.PlatformFolderName ?? ReposSettings.DefaultPlatformFolderName;
        VsCodeExecutable = settings.VSCodeExecutable ?? ReposSettings.DefaultVSCodeExecutable;
        VsCodeProfile = settings.VSCodeProfile ?? string.Empty;
        TerminalExecutable = settings.TerminalExecutable ?? ReposSettings.DefaultTerminalExecutable;
        IdeExecutable = settings.IdeExecutable ?? string.Empty;
        OpenCodeExecutable = settings.OpenCodeExecutable ?? ReposSettings.DefaultOpenCodeExecutable;
        ZCodeExecutable = settings.ZCodeExecutable ?? ReposSettings.DefaultZCodeExecutable;
        EnableTerminal = settings.EnableTerminal;
        EnableVSCode = settings.EnableVSCode;
        EnableVisualStudio = settings.EnableVisualStudio;
        EnableZCode = settings.EnableZCode;
        EnableGitHub = settings.EnableGitHub;
        GitHubExecutable = settings.GitHubExecutable ?? ReposSettings.DefaultGitHubExecutable;
        EnableAzureDevOps = settings.EnableAzureDevOps;
        AzureDevOpsPat = settings.AzureDevOpsPat ?? string.Empty;
        AzureDevOpsUrl = settings.AzureDevOpsUrl ?? string.Empty;
        MaxScanDepth = settings.MaxScanDepth > 0
            ? settings.MaxScanDepth.ToString()
            : ReposSettings.DefaultMaxScanDepth.ToString();
    }

    private static string[] ToLines(string? text)
    {
        if (string.IsNullOrWhiteSpace(text))
            return Array.Empty<string>();

        return text.Split(new[] { Environment.NewLine }, StringSplitOptions.RemoveEmptyEntries)
            .Select(l => l.Trim())
            .Where(l => !string.IsNullOrWhiteSpace(l))
            .ToArray();
    }

    private static string WithDefault(string? value, string fallback)
        => string.IsNullOrWhiteSpace(value) ? fallback : value.Trim();
}
