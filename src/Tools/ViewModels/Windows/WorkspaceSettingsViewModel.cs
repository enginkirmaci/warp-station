using CommunityToolkit.Mvvm.ComponentModel;
using Tools.Library.Configuration;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the <see cref="Views.Windows.WorkspaceSettingsDialog"/>. Holds the
/// editing state for <see cref="WorkspacesSettings"/> (multi-line text for the array
/// fields, plain strings for the rest) and translates between the two on load/save.
/// Extracted from the dialog's code-behind so editing logic lives in the ViewModel layer.
/// </summary>
public partial class WorkspaceSettingsViewModel : ObservableObject
{
    private const string DefaultGitPattern = "*.git";
    private const string DefaultSolutionPattern = "*.sln";
    private const string DefaultPlatformName = "platform";
    private const string DefaultVSCode = "code";
    private const string DefaultTerminal = "wt";
    private const int DefaultMaxScanDepth = 3;

    [ObservableProperty]
    private string _workspaceScanFoldersText = string.Empty;

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
    private string _terminalExecutable = DefaultTerminal;

    [ObservableProperty]
    private string _maxScanDepth = DefaultMaxScanDepth.ToString();

    /// <summary>
    /// Initializes a new instance from the current settings to edit.
    /// </summary>
    /// <param name="current">The current workspace settings to edit.</param>
    public WorkspaceSettingsViewModel(WorkspacesSettings current)
    {
        LoadFrom(current ?? new WorkspacesSettings());
    }

    /// <summary>
    /// Builds a <see cref="WorkspacesSettings"/> from the edited values, applying
    /// defaults for blank fields and parsing the multi-line text fields back to arrays.
    /// </summary>
    /// <returns>The edited workspace settings.</returns>
    public WorkspacesSettings BuildSettings()
    {
        return new WorkspacesSettings
        {
            WorkspaceScanFolders = ToLines(WorkspaceScanFoldersText),
            ExcludedFolders = ToLines(ExcludedFoldersText),
            GitFolderPattern = WithDefault(GitFolderPattern, DefaultGitPattern),
            SolutionFilePattern = WithDefault(SolutionFilePattern, DefaultSolutionPattern),
            PlatformFolderName = WithDefault(PlatformFolderName, DefaultPlatformName),
            VSCodeExecutable = WithDefault(VsCodeExecutable, DefaultVSCode),
            TerminalExecutable = WithDefault(TerminalExecutable, DefaultTerminal),
            MaxScanDepth = int.TryParse(MaxScanDepth, out var depth) && depth > 0 ? depth : DefaultMaxScanDepth
        };
    }

    private void LoadFrom(WorkspacesSettings settings)
    {
        WorkspaceScanFoldersText = settings.WorkspaceScanFolders is { Length: > 0 }
            ? string.Join(Environment.NewLine, settings.WorkspaceScanFolders)
            : string.Empty;

        ExcludedFoldersText = settings.ExcludedFolders is { Length: > 0 }
            ? string.Join(Environment.NewLine, settings.ExcludedFolders)
            : string.Empty;

        GitFolderPattern = settings.GitFolderPattern ?? DefaultGitPattern;
        SolutionFilePattern = settings.SolutionFilePattern ?? DefaultSolutionPattern;
        PlatformFolderName = settings.PlatformFolderName ?? DefaultPlatformName;
        VsCodeExecutable = settings.VSCodeExecutable ?? DefaultVSCode;
        TerminalExecutable = settings.TerminalExecutable ?? DefaultTerminal;
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
