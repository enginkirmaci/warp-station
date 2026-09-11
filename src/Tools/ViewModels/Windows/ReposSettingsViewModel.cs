using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;
using Tools.Services;

namespace Tools.ViewModels.Windows;

/// <summary>
/// ViewModel for the <see cref="Views.Components.Repo.ReposSettingsComponent"/> (the former
/// Repo Settings modal dialog, now hosted in the floating tool drawer). Holds the
/// editing state for <see cref="ReposSettings"/> (multi-line text for the array
/// fields, plain strings for the rest) and the OpenCode model fields (the default
/// model the quick-launch passes verbatim and the wand's commit model — moved here
/// from the OpenCode drawer, which is a per-launch surface now), translating between
/// text and settings on load/save.
/// <para>
/// The two model fields are the launch drawer's editable model ComboBoxes (one
/// <see cref="OpenCodeModelPickerViewModel"/> each over the shared
/// <c>opencode models</c> catalog): the cached catalog seeds the lists synchronously on
/// open, the fresh catalog follows async. Save resolves each picker's box text through
/// the catalog with custom ids kept verbatim.
/// </para>
/// </summary>
public partial class ReposSettingsViewModel :
    DrawerDialogViewModelBase<ReposSettingsDrawerContext, ReposSettingsEditResult>
{
    // Canonical defaults live on ReposSettings itself (the per-field Default* constants
    // that also back ReposSettings.Defaults); no private duplicates are kept here.

    /// <summary>
    /// The drawer has no depth field (the Add Repositories dialog owns that setting),
    /// so the stored value is captured on open and re-emitted on save — the caller
    /// replaces the whole Repos section with <see cref="BuildSettings"/>'s result.
    /// </summary>
    private int _preservedMaxScanDepth = ReposSettings.DefaultMaxScanDepth;

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

    /// <summary>The saved default model this open edits (the pickers' seed value).</summary>
    private string _savedDefaultModel = string.Empty;

    /// <summary>The saved wand commit model this open edits (the picker's seed value).</summary>
    private string _savedCommitModel = string.Empty;

    /// <summary>
    /// The default-model field: the launch drawer's editable model ComboBox over the
    /// shared <c>opencode models</c> catalog.
    /// </summary>
    public OpenCodeModelPickerViewModel DefaultModelPicker { get; } = new();

    /// <summary>The commit-model field (empty = the wand falls back to the default model).</summary>
    public OpenCodeModelPickerViewModel CommitModelPicker { get; } = new();

    /// <summary>The model catalog loader behind both pickers.</summary>
    private readonly IOpenCodeModelService _openCodeModelService;

    [ObservableProperty]
    private string _zCodeExecutable = ReposSettings.DefaultZCodeExecutable;

    /// <summary>The branch-name prefix pre-typed into the new-branch drawer's name field.</summary>
    [ObservableProperty]
    private string _branchNamePrefix = ReposSettings.DefaultBranchNamePrefix;

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

    /// <summary>Whether the NuGet Local tool (tools menu entry, watch chip, watch) is enabled.</summary>
    [ObservableProperty]
    private bool _enableNuget = true;

    [ObservableProperty]
    private string _azureDevOpsPat = string.Empty;

    [ObservableProperty]
    private string _azureDevOpsUrl = string.Empty;

    /// <summary>
    /// Initializes a new instance. Editing state is seeded per open through
    /// <see cref="OnDrawerContextAsync"/> (the component is resolved fresh from DI each time).
    /// </summary>
    public ReposSettingsViewModel(IToolDrawerService toolDrawer, IOpenCodeModelService openCodeModelService)
        : base(toolDrawer)
    {
        _openCodeModelService = openCodeModelService;
    }

    /// <summary>The open context's completion source the Save command resolves.</summary>
    protected override TaskCompletionSource<ReposSettingsEditResult?> GetCompletion(ReposSettingsDrawerContext context)
        => context.Completion;

    /// <inheritdoc/>
    protected override void OnDrawerContext(ReposSettingsDrawerContext context)
    {
        LoadFrom(context.Current ?? new ReposSettings());

        var openCode = context.OpenCode ?? new OpenCodeSettings();
        _savedDefaultModel = openCode.DefaultModel ?? string.Empty;
        _savedCommitModel = openCode.CommitModel ?? string.Empty;
        EnableNuget = context.NugetEnabled;
    }

    /// <summary>
    /// After the synchronous seed, loads the model catalog behind both pickers: the
    /// cached list applies synchronously ahead of the first await (the lists paint
    /// filled), then the <c>opencode models</c> refresh lands (the service's TTL decides
    /// whether the CLI actually re-runs). The guard spans both applies per picker, so the
    /// rebuilds' phantom picks never commit.
    /// </summary>
    public override async Task OnDrawerContextAsync(ReposSettingsDrawerContext? context)
    {
        await base.OnDrawerContextAsync(context);
        if (context is null)
        {
            return;
        }

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
            Log.Logger.Warning(ex, "Repo settings drawer: OpenCode model catalog load failed");
        }
    }

    /// <summary>Applies one catalog snapshot to both pickers, each seeded with the saved value it edits.</summary>
    private void ApplyCatalogs(IReadOnlyList<string> models)
    {
        DefaultModelPicker.ApplyCatalog(models, _savedDefaultModel);
        CommitModelPicker.ApplyCatalog(models, _savedCommitModel);
    }

    /// <summary>
    /// Save: resolves the drawer context with the edited settings and closes the drawer
    /// (shared confirm plumbing on the base).
    /// </summary>
    [RelayCommand]
    private void Save() => ConfirmWith(new ReposSettingsEditResult(BuildSettings(), BuildOpenCodeSettings(), EnableNuget));

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
    public OpenCodeSettings BuildOpenCodeSettings()
    {
        var defaultModel = DefaultModelPicker.ResolveCommittedValue();
        var commitModel = CommitModelPicker.ResolveCommittedValue();
        return new OpenCodeSettings
        {
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
