using CommunityToolkit.Mvvm.ComponentModel;
using Tools.Library.Configuration;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the <see cref="Views.Windows.ReposSettingsDialog"/>. Holds the
/// editing state for <see cref="ReposSettings"/> (multi-line text for the array
/// fields, plain strings for the rest) and translates between the two on load/save.
/// </summary>
public partial class ReposSettingsViewModel : ObservableObject
{
    private const string DefaultGitPattern = "*.git";
    private const string DefaultSolutionPattern = "*.sln,*.slnx";
    private const string DefaultPlatformName = "platform";
    private const string DefaultVSCode = "code";
    private const string DefaultTerminal = "wt";
    private const string DefaultOpenCode = "opencode";
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
    private string _openCodeExecutable = DefaultOpenCode;

    [ObservableProperty]
    private string _maxScanDepth = DefaultMaxScanDepth.ToString();

    /// <summary>
    /// Initializes a new instance from the current settings to edit.
    /// </summary>
    /// <param name="current">The current repo settings to edit.</param>
    public ReposSettingsViewModel(ReposSettings current)
    {
        LoadFrom(current ?? new ReposSettings());
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
            OpenCodeExecutable = WithDefault(OpenCodeExecutable, DefaultOpenCode),
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
        OpenCodeExecutable = settings.OpenCodeExecutable ?? DefaultOpenCode;
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
