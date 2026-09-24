using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.ViewModels.Windows;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the dedicated Settings page (opened from the title-bar gear; the
/// page replaces the Repositories content until the
/// back link is used). Absorbed the former Repo Settings drawer dialog: it edits the
/// full <see cref="ReposSettings"/> section (the scan patterns, executables, launch
/// toggles, GitHub/Azure DevOps columns, branch prefix), the OpenCode section — now
/// INCLUDING its enable flag, which the drawer never showed — the NuGet enable flag,
/// and the General flags (start minimized / at boot) that previously had no GUI at
/// all. One Save lands every edited section in a single settings write and then
/// live-applies them through <see cref="ReposViewModel.OnSettingsSavedAsync"/> — no
/// restart needed, same as the drawer save.
/// <para>
/// Merge discipline: the page edits every field of Repos/OpenCode/General, so those
/// sections are rebuilt outright (SortMode is carried over from the live settings —
/// the page doesn't edit it); NugetLocal is only mutated on its enable flag so the
/// watch-folder fields survive; ClipboardPassword/SnapIt are never touched.
/// </para>
/// </summary>
public partial class SettingsPageViewModel : ObservableObject
{
    private readonly ISettingsService _settingsService;
    private readonly IOpenCodeModelService _openCodeModelService;
    private readonly IProcessLauncher _processLauncher;
    private readonly INotificationService _notifications;
    private readonly IDialogService _dialogService;
    private readonly ReposViewModel _reposViewModel;
    private readonly IMainWindowProvider _mainWindowProvider;

    /// <summary>
    /// Neither the page nor the old drawer edits the Add-Repositories scan depth, so
    /// the stored value is captured on load and re-emitted on save (the caller replaces
    /// the whole Repos section with <see cref="BuildReposSettings"/>'s result).
    /// </summary>
    private int _preservedMaxScanDepth = ReposSettings.DefaultMaxScanDepth;

    /// <summary>The saved default model this load edits (the pickers' seed value).</summary>
    private string _savedDefaultModel = string.Empty;

    /// <summary>The saved wand commit model this load edits (the picker's seed value).</summary>
    private string _savedCommitModel = string.Empty;

    // ---- General ----
    [ObservableProperty]
    private bool _startMinimized;

    [ObservableProperty]
    private bool _startAtBoot;

    [ObservableProperty]
    private string _notesStorePath = string.Empty;

    [ObservableProperty]
    private bool _showTooltips = true;

    // ---- Repos: scanning ----
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

    // ---- Repos: external tools ----
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

    [ObservableProperty]
    private bool _enableTerminal = true;

    [ObservableProperty]
    private bool _enableVSCode = true;

    [ObservableProperty]
    private bool _enableVisualStudio = true;

    [ObservableProperty]
    private bool _enableZCode = true;

    // ---- Repos: Git ----
    [ObservableProperty]
    private string _branchNamePrefix = ReposSettings.DefaultBranchNamePrefix;

    // ---- Repos: columns ----
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

    // ---- OpenCode ----
    /// <summary>Whether the OpenCode integration (repo-row launch panel, wand) is surfaced.</summary>
    [ObservableProperty]
    private bool _enableOpenCode;

    /// <summary>The default-model field: the launch drawer's editable model ComboBox over the shared catalog.</summary>
    public OpenCodeModelPickerViewModel DefaultModelPicker { get; } = new();

    /// <summary>The commit-model field (empty = the wand falls back to the default model).</summary>
    public OpenCodeModelPickerViewModel CommitModelPicker { get; } = new();

    // ---- NuGet ----
    /// <summary>Whether the NuGet Local tool (tools menu entry, watch chip, watch) is enabled.</summary>
    [ObservableProperty]
    private bool _enableNuget = true;

    public SettingsPageViewModel(
        ISettingsService settingsService,
        IOpenCodeModelService openCodeModelService,
        IProcessLauncher processLauncher,
        INotificationService notifications,
        IDialogService dialogService,
        ReposViewModel reposViewModel,
        IMainWindowProvider mainWindowProvider)
    {
        _settingsService = settingsService;
        _openCodeModelService = openCodeModelService;
        _processLauncher = processLauncher;
        _notifications = notifications;
        _dialogService = dialogService;
        _reposViewModel = reposViewModel;
        _mainWindowProvider = mainWindowProvider;
    }

    /// <summary>
    /// Navigation load (the window fires it whenever the page is shown): reads every
    /// edited section into the fields, then loads the OpenCode model catalog behind
    /// both pickers — the cached list applies synchronously so the lists paint filled,
    /// the <c>opencode models</c> refresh lands async (the service's TTL decides
    /// whether the CLI actually re-runs).
    /// </summary>
    public async Task OnNavigatedToAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();

        var repos = settings.Repos ?? new ReposSettings();
        LoadFrom(repos);

        var openCode = settings.OpenCode ?? new OpenCodeSettings();
        EnableOpenCode = openCode.EnableOpenCode;
        _savedDefaultModel = openCode.DefaultModel ?? string.Empty;
        _savedCommitModel = openCode.CommitModel ?? string.Empty;

        EnableNuget = settings.NugetLocal?.EnableNuget ?? true;

        var general = settings.General ?? new GeneralSettings();
        StartMinimized = general.StartMinimized;
        StartAtBoot = general.StartAtBoot;
        NotesStorePath = general.NotesStorePath ?? string.Empty;
        ShowTooltips = general.ShowTooltips;

        try
        {
            DefaultModelPicker.BeginCatalogRefresh();
            CommitModelPicker.BeginCatalogRefresh();
            try
            {
                ApplyCatalogs(_openCodeModelService.GetCachedModels(_savedDefaultModel));

                var models = await _openCodeModelService.GetModelsAsync(OpenCodeExecutable, _savedDefaultModel);
                ApplyCatalogs(models);
            }
            finally
            {
                DefaultModelPicker.EndCatalogRefresh();
                CommitModelPicker.EndCatalogRefresh();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Settings page: OpenCode model catalog load failed");
        }
    }

    /// <summary>Applies one catalog snapshot to both pickers, each seeded with the saved value it edits.</summary>
    private void ApplyCatalogs(IReadOnlyList<string> models)
    {
        DefaultModelPicker.ApplyCatalog(models, _savedDefaultModel);
        CommitModelPicker.ApplyCatalog(models, _savedCommitModel);
    }

    /// <summary>
    /// Save: every edited section lands in one settings write (merge rules in the class
    /// header), then the live surfaces re-apply the new values through the Repos page —
    /// column flags, activity services, launch shortcuts, bottom-bar tabs, the OpenCode
    /// snapshot. StartMinimized/StartAtBoot persist only: both are reconciled against
    /// the OS registration on the next launch. ShowTooltips applies immediately on the
    /// main window.
    /// </summary>
    [RelayCommand]
    private async Task SaveAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var repos = BuildReposSettings();
            // The page doesn't edit the list sort — carry the live selection over so
            // the save doesn't revert it (the Repos toolbar owns that setting).
            repos.SortMode = settings.Repos?.SortMode ?? RepoSortMode.Name;
            settings.Repos = repos;
            settings.OpenCode = BuildOpenCodeSettings();
            settings.NugetLocal ??= new NugetLocalSettings();
            settings.NugetLocal.EnableNuget = EnableNuget;
            settings.General = new GeneralSettings
            {
                StartMinimized = StartMinimized,
                StartAtBoot = StartAtBoot,
                NotesStorePath = NotesStorePath?.Trim() ?? string.Empty,
                ShowTooltips = ShowTooltips
            };
            await _settingsService.SaveSettingsAsync(settings);

            if (_mainWindowProvider.TopLevel is { } topLevel)
            {
                ToolTip.SetServiceEnabled(topLevel, ShowTooltips);
            }

            await _reposViewModel.OnSettingsSavedAsync(
                new ReposSettingsEditResult(repos, settings.OpenCode, EnableNuget));
            _notifications.Show("Settings saved", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Settings page save failed");
            _notifications.Show("Failed to save settings", NotificationKind.Error);
        }
    }

    /// <summary>Opens the user settings folder (<c>~/.devtools</c>) in the OS file explorer
    /// (created on demand), for direct edits to keys the GUI doesn't surface.</summary>
    [RelayCommand]
    private void OpenSettingsFolder()
    {
        var settingsDirectory = UserPaths.UserDataRoot;
        Directory.CreateDirectory(settingsDirectory);
        _processLauncher.StartProcess(settingsDirectory);
    }

    /// <summary>Folder picker for the notes store path field.</summary>
    [RelayCommand]
    private async Task BrowseNotesStoreAsync()
    {
        if (await _dialogService.PickFolderAsync("Select the notes store folder") is { } folder)
        {
            NotesStorePath = folder;
        }
    }

    /// <summary>
    /// Builds a <see cref="ReposSettings"/> from the edited values, applying defaults
    /// for blank fields, parsing the multi-line text fields back to arrays, and
    /// preserving the Add-dialog's scan depth.
    /// </summary>
    private ReposSettings BuildReposSettings()
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
            BranchNamePrefix = BranchNamePrefix?.Trim() ?? string.Empty, // no default on blank: clearing disables the pre-typed prefix
            EnableTerminal = EnableTerminal,
            EnableVSCode = EnableVSCode,
            EnableVisualStudio = EnableVisualStudio,
            EnableZCode = EnableZCode,
            EnableGitHub = EnableGitHub,
            GitHubExecutable = WithDefault(GitHubExecutable, ReposSettings.DefaultGitHubExecutable),
            EnableAzureDevOps = EnableAzureDevOps,
            AzureDevOpsPat = AzureDevOpsPat?.Trim() ?? string.Empty,
            AzureDevOpsUrl = AzureDevOpsUrl?.Trim() ?? string.Empty,
            MaxScanDepth = _preservedMaxScanDepth
        };
    }

    /// <summary>
    /// Builds the edited OpenCode section: each model id resolves from its picker's box
    /// text (an exact catalog match snaps to the list's casing; a custom id is kept
    /// trimmed verbatim — an empty default is legal, quick-launch then alerts instead of
    /// guessing a model, and an empty commit model means the wand uses the default).
    /// </summary>
    private OpenCodeSettings BuildOpenCodeSettings()
    {
        var defaultModel = DefaultModelPicker.ResolveCommittedValue();
        var commitModel = CommitModelPicker.ResolveCommittedValue();
        return new OpenCodeSettings
        {
            EnableOpenCode = EnableOpenCode,
            DefaultModel = defaultModel,
            CommitModel = string.IsNullOrWhiteSpace(commitModel) ? null : commitModel
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
        BranchNamePrefix = settings.BranchNamePrefix ?? string.Empty;
        EnableTerminal = settings.EnableTerminal;
        EnableVSCode = settings.EnableVSCode;
        EnableVisualStudio = settings.EnableVisualStudio;
        EnableZCode = settings.EnableZCode;
        EnableGitHub = settings.EnableGitHub;
        GitHubExecutable = settings.GitHubExecutable ?? ReposSettings.DefaultGitHubExecutable;
        EnableAzureDevOps = settings.EnableAzureDevOps;
        AzureDevOpsPat = settings.AzureDevOpsPat ?? string.Empty;
        AzureDevOpsUrl = settings.AzureDevOpsUrl ?? string.Empty;
        _preservedMaxScanDepth = settings.MaxScanDepth > 0
            ? settings.MaxScanDepth
            : ReposSettings.DefaultMaxScanDepth;
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
