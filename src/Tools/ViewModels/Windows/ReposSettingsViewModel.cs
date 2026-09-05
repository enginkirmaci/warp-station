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
public partial class ReposSettingsViewModel : ObservableObject, IToolDrawerContextReceiver
{
    private readonly IToolDrawerService _toolDrawer;

    /// <summary>
    /// The context completion source while this instance is the drawer's open component;
    /// resolved with the edited settings on Save.
    /// </summary>
    private TaskCompletionSource<ReposSettings?>? _completion;
    private const string DefaultGitPattern = "*.git";
    private const string DefaultSolutionPattern = "*.sln,*.slnx";
    private const string DefaultPlatformName = "platform";
    private const string DefaultVSCode = "code";
    private const string DefaultTerminal = "wt";
    private const string DefaultOpenCode = "opencode";
    private const string DefaultZCode = "zcode";
    private const string DefaultGitHub = "gh";
    private const int DefaultMaxScanDepth = 3;

    [ObservableProperty]
    private string _repoScanFoldersText = string.Empty;

    [ObservableProperty]
    private string _excludedFoldersText = string.Empty;

    [ObservableProperty]
    private string _gitFolderPattern = DefaultGitPattern;

    [ObservableProperty]
    private string _solutionFilePattern = DefaultSolutionPattern;

    [ObservableProperty]
    private string _platformFolderName = DefaultPlatformName;

    [ObservableProperty]
    private string _vsCodeExecutable = DefaultVSCode;

    [ObservableProperty]
    private string _vsCodeProfile = string.Empty;

    [ObservableProperty]
    private string _terminalExecutable = DefaultTerminal;

    [ObservableProperty]
    private string _ideExecutable = string.Empty;

    [ObservableProperty]
    private string _openCodeExecutable = DefaultOpenCode;

    [ObservableProperty]
    private string _zCodeExecutable = DefaultZCode;

    [ObservableProperty]
    private bool _showGitHubColumn = true;

    [ObservableProperty]
    private string _gitHubExecutable = DefaultGitHub;

    [ObservableProperty]
    private bool _showAzureDevOpsColumn = true;

    [ObservableProperty]
    private string _azureDevOpsPat = string.Empty;

    [ObservableProperty]
    private string _maxScanDepth = DefaultMaxScanDepth.ToString();

    /// <summary>
    /// Initializes a new instance. Editing state is seeded per open through
    /// <see cref="OnDrawerContext"/> (the component is resolved fresh from DI each time).
    /// </summary>
    public ReposSettingsViewModel(IToolDrawerService toolDrawer)
    {
        _toolDrawer = toolDrawer;
    }

    /// <summary>
    /// Drawer open payload: the settings instance to edit plus the completion source the
    /// Save command resolves. Re-loads the fields so every open starts from the caller's
    /// current settings.
    /// </summary>
    public void OnDrawerContext(object context)
    {
        if (context is not ReposSettingsDrawerContext drawerContext)
        {
            return;
        }

        _completion = drawerContext.Completion;
        LoadFrom(drawerContext.Current ?? new ReposSettings());
    }

    /// <summary>Cancel/close: resolves the context with null (DialogService semantics).</summary>
    [RelayCommand]
    private void Cancel() => _toolDrawer.Close();

    /// <summary>
    /// Save: resolves the drawer context with the edited settings and closes the drawer.
    /// Closing raises the drawer's Changed event, which DialogService treats as a cancel
    /// — a no-op here because the completion is already set.
    /// </summary>
    [RelayCommand]
    private void Save()
    {
        _completion?.TrySetResult(BuildSettings());
        _toolDrawer.Close();
    }

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
            GitFolderPattern = WithDefault(GitFolderPattern, DefaultGitPattern),
            SolutionFilePattern = WithDefault(SolutionFilePattern, DefaultSolutionPattern),
            PlatformFolderName = WithDefault(PlatformFolderName, DefaultPlatformName),
            VSCodeExecutable = WithDefault(VsCodeExecutable, DefaultVSCode),
            // No default: empty means "open VS Code with the default profile".
            VSCodeProfile = VsCodeProfile?.Trim() ?? string.Empty,
            TerminalExecutable = WithDefault(TerminalExecutable, DefaultTerminal),
            // No default: empty means "auto-detect the IDE" (or use the .sln association on Windows).
            IdeExecutable = IdeExecutable?.Trim() ?? string.Empty,
            OpenCodeExecutable = WithDefault(OpenCodeExecutable, DefaultOpenCode),
            ZCodeExecutable = WithDefault(ZCodeExecutable, DefaultZCode),
            ShowGitHubColumn = ShowGitHubColumn,
            GitHubExecutable = WithDefault(GitHubExecutable, DefaultGitHub),
            ShowAzureDevOpsColumn = ShowAzureDevOpsColumn,
            AzureDevOpsPat = AzureDevOpsPat?.Trim() ?? string.Empty,
            MaxScanDepth = int.TryParse(MaxScanDepth, out var depth) && depth > 0 ? depth : DefaultMaxScanDepth
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

        GitFolderPattern = settings.GitFolderPattern ?? DefaultGitPattern;
        SolutionFilePattern = settings.SolutionFilePattern ?? DefaultSolutionPattern;
        PlatformFolderName = settings.PlatformFolderName ?? DefaultPlatformName;
        VsCodeExecutable = settings.VSCodeExecutable ?? DefaultVSCode;
        VsCodeProfile = settings.VSCodeProfile ?? string.Empty;
        TerminalExecutable = settings.TerminalExecutable ?? DefaultTerminal;
        IdeExecutable = settings.IdeExecutable ?? string.Empty;
        OpenCodeExecutable = settings.OpenCodeExecutable ?? DefaultOpenCode;
        ZCodeExecutable = settings.ZCodeExecutable ?? DefaultZCode;
        ShowGitHubColumn = settings.ShowGitHubColumn;
        GitHubExecutable = settings.GitHubExecutable ?? DefaultGitHub;
        ShowAzureDevOpsColumn = settings.ShowAzureDevOpsColumn;
        AzureDevOpsPat = settings.AzureDevOpsPat ?? string.Empty;
        MaxScanDepth = settings.MaxScanDepth > 0 ? settings.MaxScanDepth.ToString() : DefaultMaxScanDepth.ToString();
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
