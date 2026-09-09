using System.Collections.ObjectModel;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Helpers;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Services.Abstractions;
using Tools.ViewModels.Components;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.ViewModels.Windows;

/// <summary>
/// The open payload for the OpenCode drawer component: the repo the settings/launch
/// act on (the clicked row's repo, or the bar's selected repo when opened from the
/// tools dropdown; null when nothing is selected).
/// </summary>
public sealed record OpenCodeSettingsContext(Repo? Repo);

/// <summary>
/// The OpenCode launch drawer: a per-launch model picker, instances, template, prompt
/// and the launch button — the former OpenCode bottom panel, redesigned in the drawer's
/// card vocabulary. Nothing here persists: the saved default model is edited in the
/// Repo Settings drawer; the pick made here only selects the model for the instances
/// this panel launches (seeded from the saved default on open). Settings are read
/// fresh per open — this VM is transient, so all UI state seeds through
/// <see cref="OnDrawerContextAsync"/>.
/// <para>
/// The editable model ComboBox commits its selection through the component's code-behind
/// instead of a TwoWay binding so the in-place ItemsSource rebuilds (model list refresh)
/// never write a transient null back into this VM.
/// </para>
/// </summary>
public partial class OpenCodeSettingsViewModel : ObservableObject, IToolDrawerContextReceiver<OpenCodeSettingsContext>
{
    private readonly ISettingsService _settingsService;
    private readonly IOpenCodeModelService _openCodeModelService;
    private readonly IOpenCodeTemplateService _openCodeTemplateService;
    private readonly IOpenCodePromptService _openCodePromptService;
    private readonly IOpenCodeGridLauncher _openCodeGridLauncher;
    private readonly ITerminalLauncher _terminalLauncher;
    private readonly INotificationService _notificationService;
    private readonly IToolDrawerService _toolDrawer;
    private readonly BottomBarViewModel _bottomBar;

    private ReposSettings _reposSettings = new();

    /// <summary>The repo the settings/launch act on (null = nothing selected yet).</summary>
    private Repo? _repo;

    public OpenCodeSettingsViewModel(
        ISettingsService settingsService,
        IOpenCodeModelService openCodeModelService,
        IOpenCodeTemplateService openCodeTemplateService,
        IOpenCodePromptService openCodePromptService,
        IOpenCodeGridLauncher openCodeGridLauncher,
        ITerminalLauncher terminalLauncher,
        INotificationService notificationService,
        IToolDrawerService toolDrawer,
        BottomBarViewModel bottomBar)
    {
        _settingsService = settingsService;
        _openCodeModelService = openCodeModelService;
        _openCodeTemplateService = openCodeTemplateService;
        _openCodePromptService = openCodePromptService;
        _openCodeGridLauncher = openCodeGridLauncher;
        _terminalLauncher = terminalLauncher;
        _notificationService = notificationService;
        _toolDrawer = toolDrawer;
        _bottomBar = bottomBar;
    }

    /// <summary>Whether the OpenCode integration is enabled (seeded per open).</summary>
    [ObservableProperty]
    private bool _hasOpenCode;

    /// <summary>The launch/settings target's name, for the card header line.</summary>
    [ObservableProperty]
    private string _targetRepoName = "No repository selected";

    // --- Model defaults ---

    /// <summary>The full model catalog (cache + <c>opencode models</c> refresh).</summary>
    [ObservableProperty]
    private ObservableCollection<string> _openCodeModels = new();

    /// <summary>
    /// The currently selected model. Bound OneWay so the box genuinely highlights the
    /// configured default; user picks are committed by the component's SelectionChanged
    /// handler (see <see cref="CommitModelPick"/>), not by a TwoWay binding — a TwoWay
    /// writeback would null the selection during the in-place list rebuilds.
    /// </summary>
    [ObservableProperty]
    private string _openCodeSelectedModel = string.Empty;

    /// <summary>The editable ComboBox's live search text, kept separate from the committed selection.</summary>
    [ObservableProperty]
    private string _openCodeModelFilter = string.Empty;

    /// <summary>The dropdown list: <see cref="OpenCodeModels"/> filtered by <see cref="OpenCodeModelFilter"/>.</summary>
    [ObservableProperty]
    private ObservableCollection<string> _openCodeFilteredModels = new();

    public bool OpenCodeHasModels => OpenCodeModels.Count > 0;
    public bool OpenCodeModelsEmpty => OpenCodeModels.Count == 0;

    /// <summary>
    /// Loads the model list: the cached list shows immediately, then the catalog refreshes —
    /// the <c>opencode models</c> CLI only re-runs when the service's in-memory catalog is
    /// staler than its TTL, so repeated drawer opens don't respawn the process. Called on
    /// every drawer open.
    /// </summary>
    private async Task LoadModelsAsync()
    {
        // Guard spans the WHOLE load — including the await and the deferred
        // SelectionChanged the ComboBox raises after its ItemsSource swap (a synchronous
        // flag reset misses that, and the phantom pick would persist over the configured
        // default on every open).
        _syncingPickers = true;
        try
        {
            var cached = _openCodeModelService.GetCachedModels(_openCodeSettings.DefaultModel);
            if (cached.Count > 0)
                ApplyModels(cached);

            var models = await _openCodeModelService.GetModelsAsync(_reposSettings.OpenCodeExecutable, _openCodeSettings.DefaultModel);
            ApplyModels(models);
        }
        finally
        {
            Dispatcher.UIThread.Post(() => _syncingPickers = false, DispatcherPriority.Background);
        }
    }

    /// <summary>
    /// True while <see cref="LoadModelsAsync"/> refreshes the model list: the ItemsSource
    /// swap transiently re-selects entries and re-fires the default-model picker's commit
    /// handler, which must not persist those phantom picks.
    /// </summary>
    private bool _syncingPickers;

    /// <summary>The OpenCode settings snapshot this open seeds from and writes against.</summary>
    private OpenCodeSettings _openCodeSettings = new();

    /// <summary>
    /// Pushes <paramref name="models"/> into <see cref="OpenCodeModels"/>, selects the
    /// configured default or first entry (or clears the selection when empty), and
    /// refreshes the filter projection and the computed has/empty flags. A model the user
    /// already picked survives the refresh when it is still present. Runs inside the
    /// <see cref="_syncingPickers"/> guard.
    /// </summary>
    private void ApplyModels(IReadOnlyList<string> models)
    {
        OpenCodeModels = new ObservableCollection<string>(models);

        var previous = OpenCodeSelectedModel;
        var previousStillListed = !string.IsNullOrWhiteSpace(previous) && OpenCodeModels.Contains(previous);
        OpenCodeSelectedModel = previousStillListed
            ? previous
            : SelectConfiguredOrDefaultModel(OpenCodeModels);

        OpenCodeModelFilter = OpenCodeSelectedModel;
        RefreshFilteredModels();

        // Re-raise so the OneWay SelectedItem binding re-resolves after the in-place list
        // rebuild — including when the value did not change and ObservableProperty raised
        // nothing. Safe from text clobbering: the filter was just mirrored to the same
        // value, and the commit handler re-commits equal values (no loop).
        OnPropertyChanged(nameof(OpenCodeSelectedModel));

        OnPropertyChanged(nameof(OpenCodeHasModels));
        OnPropertyChanged(nameof(OpenCodeModelsEmpty));
    }

    /// <summary>
    /// The model to preselect (and launch) when the user has not picked one: the
    /// configured default when set and listed — matched case-insensitively and resolved
    /// to the list's own casing — otherwise the first model. Delegates to the model
    /// service, which owns the rule (and applies it to the catalog's ordering too).
    /// </summary>
    private string SelectConfiguredOrDefaultModel(IReadOnlyList<string> models)
        => _openCodeModelService.ResolveLaunchModel(models, _openCodeSettings.DefaultModel);

    /// <summary>
    /// The model to launch with, in priority order: an exact match for what the box
    /// shows, the committed dropdown selection, and finally the configured default or
    /// the first model.
    /// </summary>
    private string ResolveLaunchModel()
    {
        var typed = OpenCodeModelFilter?.Trim();
        var typedMatch = string.IsNullOrWhiteSpace(typed)
            ? null
            : OpenCodeModels.FirstOrDefault(m => string.Equals(m, typed, StringComparison.OrdinalIgnoreCase));

        if (typedMatch is not null)
        {
            return typedMatch;
        }

        return string.IsNullOrWhiteSpace(OpenCodeSelectedModel)
            ? SelectConfiguredOrDefaultModel(OpenCodeModels)
            : OpenCodeSelectedModel;
    }

    /// <summary>
    /// Commits a model picked from the dropdown (called by the component's code-behind):
    /// updates the selection and filter so the next launch from this panel uses it. The
    /// pick is deliberately NOT persisted — the saved default model is edited in the
    /// Repo Settings drawer; this picker is launch-scoped.
    /// </summary>
    public void CommitModelPick(string model)
    {
        if (_syncingPickers) return; // phantom pick from an option-list rebuild
        if (string.IsNullOrWhiteSpace(model)) return;

        OpenCodeSelectedModel = model;
        OpenCodeModelFilter = model;
    }

    // --- Launch options ---

    [ObservableProperty]
    private ObservableCollection<OpenCodeTemplate> _openCodeTemplates = new() { OpenCodeTemplate.None };

    [ObservableProperty]
    private OpenCodeTemplate _openCodeSelectedTemplate = OpenCodeTemplate.None;

    public string OpenCodeSelectedTemplateDescription => OpenCodeSelectedTemplate?.Description ?? string.Empty;

    [ObservableProperty]
    private ObservableCollection<OpenCodePromptEntry> _openCodePrompts = new() { OpenCodePromptEntry.None };

    [ObservableProperty]
    private OpenCodePromptEntry _openCodeSelectedPrompt = OpenCodePromptEntry.None;

    [ObservableProperty]
    private string _openCodePrompt = string.Empty;

    [ObservableProperty]
    private string _newPromptName = string.Empty;

    [ObservableProperty]
    private int _openCodeInstanceCount = 1;

    /// <summary>
    /// Whether to tile the launched opencode instances across the screen in a grid.
    /// Off by default — instances open as plain terminal windows; checking it routes the
    /// launch through <see cref="IOpenCodeGridLauncher"/>.
    /// </summary>
    [ObservableProperty]
    private bool _openCodeArrangeIntoGrid;

    /// <summary>
    /// Whether the "Arrange into grid" checkbox offers itself. The grid launcher positions
    /// windows through SnapIt's Win32 primitives, so the option only exists on Windows.
    /// Runtime check, never the build-OS constant.
    /// </summary>
    public bool CanArrangeIntoGrid => OperatingSystem.IsWindows();

    partial void OnOpenCodeSelectedPromptChanged(OpenCodePromptEntry value)
    {
        if (value is null || value.IsNone)
            return;
        OpenCodePrompt = value.Prompt;
    }

    /// <summary>
    /// Saves the current Start prompt under the name in <see cref="NewPromptName"/>, reloads
    /// the selector and selects the saved entry.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanSavePrompt))]
    private async Task SavePromptAsync()
    {
        var name = (NewPromptName ?? string.Empty).Trim();
        if (string.IsNullOrEmpty(name)) return;

        await _openCodePromptService.SaveAsync(name, OpenCodePrompt ?? string.Empty);
        NewPromptName = string.Empty;

        await LoadPromptsAsync();

        OpenCodeSelectedPrompt = OpenCodePrompts.FirstOrDefault(p =>
            string.Equals(p.Name, name, StringComparison.OrdinalIgnoreCase)) ?? OpenCodePromptEntry.None;
        _notificationService.Show("Prompt saved", NotificationKind.Success);
    }

    private bool CanSavePrompt()
        => !string.IsNullOrWhiteSpace(NewPromptName) && !string.IsNullOrWhiteSpace(OpenCodePrompt);

    partial void OnNewPromptNameChanged(string value) => SavePromptCommand.NotifyCanExecuteChanged();
    partial void OnOpenCodePromptChanged(string value) => SavePromptCommand.NotifyCanExecuteChanged();

    private async Task LoadPromptsAsync()
    {
        var prompts = await _openCodePromptService.LoadAsync();
        var collection = new ObservableCollection<OpenCodePromptEntry> { OpenCodePromptEntry.None };
        foreach (var prompt in prompts)
            collection.Add(prompt);
        OpenCodePrompts = collection;
    }

    /// <summary>
    /// Removes the target repo's <c>.opencode</c> folder and re-copies the currently
    /// selected template into it, without launching OpenCode.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanResetTemplate))]
    private async Task ResetOpenCodeTemplateAsync()
    {
        var repo = _repo;
        if (repo?.FolderPath is null || OpenCodeSelectedTemplate.IsNone)
            return;

        await _openCodeTemplateService.CopyToRepoAsync(OpenCodeSelectedTemplate, repo.FolderPath);
        _notificationService.Show("Template reset", NotificationKind.Success);
    }

    private bool CanResetTemplate()
        => _repo?.FolderPath is not null && !OpenCodeSelectedTemplate.IsNone;

    /// <summary>
    /// Re-evaluate <see cref="ResetOpenCodeTemplateCommand"/>, refresh the computed
    /// description binding, and coerce transient nulls (the ComboBox TwoWay binding pushes
    /// null when <see cref="OpenCodeTemplates"/> is swapped) back to the None sentinel.
    /// </summary>
    partial void OnOpenCodeSelectedTemplateChanged(OpenCodeTemplate value)
    {
        if (value is null)
        {
            OpenCodeSelectedTemplate = OpenCodeTemplate.None;
            return;
        }
        ResetOpenCodeTemplateCommand.NotifyCanExecuteChanged();
        OnPropertyChanged(nameof(OpenCodeSelectedTemplateDescription));
    }

    /// <summary>
    /// Launches opencode in the target repo with the current options (model, instances,
    /// grid, template, prompt) and closes the drawer once the instances are on their way.
    /// The plain-window launch path (and the command-line assembly it shares with the
    /// grid) lives in <see cref="ITerminalLauncher"/>; this VM keeps only the terminal
    /// guard — the grid branch still needs the resolved executables.
    /// </summary>
    [RelayCommand]
    private async Task LaunchOpenCodeAsync()
    {
        var repo = _repo;
        if (repo?.FolderPath is null || !_bottomBar.HasOpenCode) return;

        // Copy the selected template (if any) to <repo>/.opencode before launching.
        await _openCodeTemplateService.CopyToRepoAsync(OpenCodeSelectedTemplate, repo.FolderPath);

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null)
        {
            _notificationService.Show("No known terminal found — set the terminal executable in Repos settings", NotificationKind.Error);
            _toolDrawer.Close();
            return;
        }

        var openCodeExe = ExecutableDefaults.ResolveCliForTerminal(_reposSettings.OpenCodeExecutable, ReposSettings.DefaultOpenCodeExecutable);
        var prompt = OpenCodePrompt?.Trim();
        var count = OpenCodeInstanceCount < 1 ? 1 : OpenCodeInstanceCount;
        var model = ResolveLaunchModel();

        if (OpenCodeArrangeIntoGrid)
        {
            await _openCodeGridLauncher.LaunchAsync(terminalExe, openCodeExe, repo.FolderPath, model, prompt ?? string.Empty, count);
        }
        else
        {
            _terminalLauncher.LaunchOpenCode(terminalExe, openCodeExe, repo.FolderPath, model, prompt ?? string.Empty, count);
        }

        _toolDrawer.Close();
    }

    /// <summary>
    /// Rebuilds <see cref="OpenCodeFilteredModels"/> from <see cref="OpenCodeModels"/>
    /// using the current filter. Must not run synchronously from a filter writeback that
    /// originates inside the ComboBox's own selection update — see
    /// <see cref="ScheduleFilteredModelsRefresh"/>.
    /// </summary>
    private void RefreshFilteredModels()
    {
        var filter = OpenCodeModelFilter ?? string.Empty;
        bool isFullSelection = string.IsNullOrEmpty(filter)
            || string.Equals(filter, OpenCodeSelectedModel, StringComparison.Ordinal);
        var source = (isFullSelection
            ? OpenCodeModels
            : OpenCodeModels.Where(m => m.Contains(filter, StringComparison.OrdinalIgnoreCase))).ToList();

        // Rebuild in place rather than swapping in a new instance: the ComboBox's Text
        // binding raises the filter change from inside the control's own selection
        // update, and re-sourcing ItemsSource there throws "Cannot change source while
        // update is in progress". Skip the rebuild entirely when the projection already
        // matches — Clear() raises a Reset which drops the control-side selection even
        // when the content is identical.
        if (source.Count == OpenCodeFilteredModels.Count && source.SequenceEqual(OpenCodeFilteredModels))
            return;

        OpenCodeFilteredModels.Clear();
        foreach (var model in source)
            OpenCodeFilteredModels.Add(model);
    }

    /// <summary>Coalesces the deferred <see cref="RefreshFilteredModels"/> passes —
    /// one queued dispatcher pass per burst.</summary>
    private readonly UiPostOnce _filteredModelsRefreshPost = new();

    partial void OnOpenCodeModelFilterChanged(string value) => ScheduleFilteredModelsRefresh();

    /// <summary>
    /// Schedules <see cref="RefreshFilteredModels"/> on the next dispatcher pass,
    /// coalescing bursts into one rebuild. The deferral is load-bearing: mutating the
    /// filtered list synchronously from the Text writeback raises CollectionChanged
    /// re-entrantly inside the ComboBox's selection update and the selection model throws.
    /// </summary>
    private void ScheduleFilteredModelsRefresh()
    {
        _filteredModelsRefreshPost.Post(() =>
        {
            // Capture whether the box is supposed to be showing the committed selection
            // before rebuilding — while a user search is in flight the filter differs.
            bool boxShowsSelection = string.Equals(OpenCodeModelFilter, OpenCodeSelectedModel, StringComparison.Ordinal);
            RefreshFilteredModels();

            // A rebuild that actually runs drops the ComboBox's control-side selection;
            // when the box was showing the committed selection, re-push it so the OneWay
            // SelectedItem binding re-resolves and reselects the entry.
            if (boxShowsSelection && !string.IsNullOrEmpty(OpenCodeSelectedModel))
                OnPropertyChanged(nameof(OpenCodeSelectedModel));
        });
    }

    partial void OnOpenCodeModelsChanged(ObservableCollection<string> value)
        => ScheduleFilteredModelsRefresh();

    private async Task LoadTemplatesAsync()
    {
        var templates = await _openCodeTemplateService.LoadAsync();
        var collection = new ObservableCollection<OpenCodeTemplate> { OpenCodeTemplate.None };
        foreach (var template in templates)
            collection.Add(template);
        OpenCodeTemplates = collection;
    }

    /// <summary>
    /// Drawer open payload: the repo the settings/launch act on (null when the open
    /// carried none — the tools-dropdown opens seed from the bar's selection instead).
    /// Every delivery reloads the fresh settings, models, templates and prompts.
    /// </summary>
    public async Task OnDrawerContextAsync(OpenCodeSettingsContext? context)
    {
        try
        {
            _repo = context?.Repo;

            // Fresh, authoritative values: GetSettingsAsync returns a copy, so the previous
            // open's edits are visible here even though this VM instance is brand new.
            var settings = await _settingsService.GetSettingsAsync();
            _reposSettings = settings.Repos ?? new ReposSettings();
            _openCodeSettings = settings.OpenCode ?? new OpenCodeSettings();

            HasOpenCode = _bottomBar.HasOpenCode;
            TargetRepoName = _repo?.Name ?? "No repository selected";

            await LoadModelsAsync();
            await LoadTemplatesAsync();
            await LoadPromptsAsync();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCode settings drawer: seeding failed");
        }
    }
}
