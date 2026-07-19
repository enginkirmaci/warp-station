using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Formatters;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;
using Tools.Services;
using Tools.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// Binding adapter for the Repos page. Delegates scanning, caching, and the shared
/// repo state to <see cref="IRepoService"/> (singleton), process launching to
/// <see cref="IProcessLauncher"/>, and tag persistence back through the service.
/// Holds only view-specific state: the text + tag filters, the filtered projection,
/// and the transient OpenCode panel options.
/// </summary>
public partial class ReposViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly IDevToolsClient _devToolsClient;
    private readonly IDialogService _dialogService;
    private readonly IRepoService _repoService;
    private readonly IGitStatusService _gitStatusService;
    private readonly IProcessLauncher _processLauncher;
    private readonly IOpenCodeTemplateService _openCodeTemplateService;
    private readonly IOpenCodePromptService _openCodePromptService;
    private readonly IOpenCodeGridLauncher _openCodeGridLauncher;
    private readonly IOpenCodeServeService _openCodeServeService;
    private readonly INotificationService _notificationService;
    private ReposSettings _reposSettings = new();
    private OpenCodeServeSettings _openCodeServeSettings = new();

    /// <summary>
    /// The model selected by default when the OpenCode panel opens, resolved from the
    /// connected <c>opencode serve</c> instance. Empty (no default) until serve is connected.
    /// </summary>
    private string _defaultOpenCodeModel = string.Empty;

    [ObservableProperty]
    private string _filterText = string.Empty;

    [ObservableProperty]
    private ObservableCollection<Repo> _filteredRepos = new();

    /// <summary>
    /// The checkable tag list shown in the left filter panel. Rebuilt from
    /// <see cref="IRepoService.AllTags"/> whenever the service changes, preserving
    /// existing check states by tag name so checking a tag survives a rescan.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<TagFilter> _tagFilters = new();

    // --- OpenCode panel (transient state) ---

    /// <summary>
    /// Whether the OpenCode integration is enabled (mirrors <see cref="OpenCodeServeSettings.Enabled"/>).
    /// When false, serve is not started, the launch panel cannot open, and all
    /// per-repo OpenCode UI is hidden.
    /// </summary>
    [ObservableProperty]
    private bool _isOpenCodeEnabled;

    [ObservableProperty]
    private bool _isOpenCodePanelOpen;

    [ObservableProperty]
    private Repo? _openCodeRepo;

    /// <summary>
    /// A null-safe computed view of <see cref="OpenCodeRepo"/>'s name for binding.
    /// Binding directly to <c>OpenCodeRepo.Name</c> fails to traverse the path when
    /// <see cref="OpenCodeRepo"/> is null (panel closed); this wrapper avoids that.
    /// </summary>
    public string OpenCodeRepoName => OpenCodeRepo?.Name ?? string.Empty;

    [ObservableProperty]
    private int _openCodeInstanceCount = 1;

    /// <summary>
    /// When <see langword="true"/>, launching multiple OpenCode instances tiles them
    /// across the active screen in a grid (e.g. 6 instances -> a 3x2 grid). When
    /// <see langword="false"/>, instances open without positioning, as before.
    /// <para>
    /// Defaults to <see langword="false"/> (a single instance needs no tiling). It is
    /// auto-enabled the moment <see cref="OpenCodeInstanceCount"/> rises above 1; the user
    /// can still turn it off manually afterwards.
    /// </para>
    /// </summary>
    [ObservableProperty]
    private bool _arrangeInGrid;

    /// <summary>
    /// When <see langword="true"/>, the instance launched from the panel is registered with
    /// auto-approve: the serve service replies <c>"once"</c> to each of its
    /// <c>permission.updated</c> events. Defaults from <c>OpenCodeServeSettings.AutoApprove</c>
    /// when the panel opens, and is reset when the panel closes.
    /// </summary>
    [ObservableProperty]
    private bool _openCodeAutoApprove;

    /// <summary>
    /// The models available in the OpenCode model selector, fetched live from the connected
    /// <c>opencode serve</c> instance (<c>GET /config/providers</c>). Empty while serve is
    /// not connected; (re)populated when the panel opens or when serve connects.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<string> _openCodeModels = new();

    /// <summary>
    /// The currently selected model in the OpenCode panel. Passed to opencode via
    /// <c>opencode model "&lt;model&gt;"</c>. Driven by the editable model ComboBox's selection,
    /// not its text — see <see cref="OpenCodeModelFilter"/>.
    /// </summary>
    [ObservableProperty]
    private string _openCodeSelectedModel = string.Empty;

    /// <summary>
    /// The text the user is typing into the editable model ComboBox. This is the live search
    /// term, kept separate from <see cref="OpenCodeSelectedModel"/> so filtering the dropdown
    /// never overwrites the committed selection. Reset to the selected model's full name after a
    /// pick (see <see cref="OnOpenCodeModelSelectionChanged"/> in the page code-behind).
    /// </summary>
    [ObservableProperty]
    private string _openCodeModelFilter = string.Empty;

    /// <summary>
    /// The model list shown in the editable ComboBox's dropdown: <see cref="OpenCodeModels"/>
    /// filtered by <see cref="OpenCodeModelFilter"/> (case-insensitive <c>Contains</c>). When the
    /// filter is empty the full list is shown, so opening the dropdown lists every model.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<string> _openCodeFilteredModels = new();

    /// <summary>
    /// The templates available in the OpenCode template selector, loaded from
    /// <c>settings/opencode/templates/</c>. The first entry is always the
    /// <see cref="OpenCodeTemplate.None"/> sentinel, so the selector is optional.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<OpenCodeTemplate> _openCodeTemplates = new() { OpenCodeTemplate.None };

    /// <summary>
    /// The currently selected template in the OpenCode panel. Defaults to
    /// <see cref="OpenCodeTemplate.None"/> (no template). On launch the selected template's
    /// folder is copied to <c>&lt;repo&gt;/.opencode</c>. Templates no longer carry a
    /// prompt — see <see cref="OpenCodePrompts"/>.
    /// </summary>
    [ObservableProperty]
    private OpenCodeTemplate _openCodeSelectedTemplate = OpenCodeTemplate.None;

    /// <summary>
    /// A null-safe computed view of the selected template's description for binding, avoiding
    /// a null-intermediate path traversal on <see cref="OpenCodeSelectedTemplate"/>.
    /// </summary>
    public string OpenCodeSelectedTemplateDescription => OpenCodeSelectedTemplate?.Description ?? string.Empty;

    /// <summary>
    /// The prompts available in the OpenCode prompt selector, loaded from
    /// <c>settings/opencode/prompts.json</c>. The first entry is always the
    /// <see cref="OpenCodePromptEntry.None"/> sentinel, so the selector is optional.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<OpenCodePromptEntry> _openCodePrompts = new() { OpenCodePromptEntry.None };

    /// <summary>
    /// The currently selected prompt in the OpenCode panel. Defaults to
    /// <see cref="OpenCodePromptEntry.None"/> (no prompt). When a real prompt is picked,
    /// its text is loaded into <see cref="OpenCodePrompt"/>; the user can still edit it
    /// before launching.
    /// </summary>
    [ObservableProperty]
    private OpenCodePromptEntry _openCodeSelectedPrompt = OpenCodePromptEntry.None;

    [ObservableProperty]
    private string _openCodePrompt = string.Empty;

    /// <summary>
    /// Bound to the save-prompt flyout TextBox (the name to save the current Start prompt
    /// under); cleared after a prompt is saved.
    /// </summary>
    [ObservableProperty]
    private string _newPromptName = string.Empty;

    /// <summary>
    /// Bound to the add-tag flyout TextBox; cleared after a tag is added.
    /// </summary>
    [ObservableProperty]
    private string _newTagText = string.Empty;

    /// <summary>
    /// Existing tags the user can quickly add from the add-tag flyout (everything
    /// currently in use, minus the auto-tag <c>platform</c> since it is auto-assigned).
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<string> _availableTags = new();

    public ReposViewModel(
        ISettingsService settingsService,
        IDevToolsClient devToolsClient,
        IDialogService dialogService,
        IRepoService repoService,
        IGitStatusService gitStatusService,
        IProcessLauncher processLauncher,
        IOpenCodeTemplateService openCodeTemplateService,
        IOpenCodePromptService openCodePromptService,
        IOpenCodeGridLauncher openCodeGridLauncher,
        IOpenCodeServeService openCodeServeService,
        INotificationService notificationService)
    {
        _settingsService = settingsService;
        _devToolsClient = devToolsClient;
        _dialogService = dialogService;
        _repoService = repoService;
        _gitStatusService = gitStatusService;
        _processLauncher = processLauncher;
        _openCodeTemplateService = openCodeTemplateService;
        _openCodePromptService = openCodePromptService;
        _openCodeGridLauncher = openCodeGridLauncher;
        _openCodeServeService = openCodeServeService;
        _notificationService = notificationService;

        _repoService.Changed += OnRepoChanged;
        // Refresh the serve-backed model list when serve (re)connects while the panel is open,
        // so the dropdown populates the moment a previously-down serve comes up.
        _openCodeServeService.ConnectionChanged += OnServeConnectionChanged;
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override Task OnNavigatedFromAsync()
    {
        // Detach from the singletons so this Transient VM (rebuilt per navigation) is not
        // kept alive by them and does not receive further state changes.
        _repoService.Changed -= OnRepoChanged;
        _openCodeServeService.ConnectionChanged -= OnServeConnectionChanged;
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        _reposSettings = settings.Repos ?? new ReposSettings();
        _openCodeServeSettings = settings.OpenCode ?? new OpenCodeServeSettings();
        IsOpenCodeEnabled = _openCodeServeSettings.Enabled;
        await _repoService.EnsureLoadedAsync(_reposSettings);
        await LoadOpenCodeTemplatesAsync();
        await LoadOpenCodePromptsAsync();
        RebuildTagFilters();
        ApplyFilter();

        // Kick the local git status checks in the background — the cards render instantly
        // with a "checking…" placeholder and the counts fill in as each repo's probe
        // completes. Fire-and-forget so page load never waits on git. (The service also
        // self-refreshes whenever IRepoService reports fresh data; this call covers the
        // first navigation, where the singleton may have missed the initial scan events.)
        _ = _gitStatusService.RefreshAllAsync();

        // Auto-start + connect the opencode serve subprocess once Repos is open, so the status
        // bar reflects a live connection and instances can be tracked. Idempotent: a no-op if
        // already running (e.g. auto-started by MainWindowViewModel at app launch). Skipped
        // entirely when the OpenCode integration is disabled (serve is not needed).
        if (IsOpenCodeEnabled)
        {
            await _openCodeServeService.EnsureStartedAsync(_openCodeServeSettings);
        }
    }

    /// <summary>
    /// Loads the OpenCode model list from the connected <c>opencode serve</c> instance
    /// (<c>GET /config/providers</c>) into <see cref="OpenCodeModels"/> and resolves the
    /// default selection. Returns an empty list when serve is not connected, so the selector
    /// shows its "connect serve" hint. Safe to call before the panel opens; re-called on serve
    /// connect (see <see cref="OnServeConnectionChanged"/>) and on panel open.
    /// </summary>
    private async Task LoadOpenCodeModelsFromServeAsync()
    {
        var result = await _openCodeServeService.GetModelsAsync();
        OpenCodeModels = new ObservableCollection<string>(result.Models);
        _defaultOpenCodeModel = string.IsNullOrWhiteSpace(result.DefaultModel) && OpenCodeModels.Count > 0
            ? OpenCodeModels[0]
            : result.DefaultModel;

        // Preserve the current selection when it is still offered; otherwise fall back to the
        // serve default (or clear it when nothing is available).
        if (OpenCodeModels.Count == 0)
            OpenCodeSelectedModel = string.Empty;
        else if (string.IsNullOrEmpty(OpenCodeSelectedModel)
                 || !OpenCodeModels.Contains(OpenCodeSelectedModel, StringComparer.Ordinal))
            OpenCodeSelectedModel = _defaultOpenCodeModel;

        // The editable ComboBox binds its text to the filter and its dropdown to the filtered
        // list; mirror the committed selection into both so the box shows the active model.
        OpenCodeModelFilter = OpenCodeSelectedModel;
        RefreshOpenCodeFilteredModels();

        // The computed has/empty flags feed the ComboBox IsEnabled and the hint visibility.
        OnPropertyChanged(nameof(OpenCodeHasModels));
        OnPropertyChanged(nameof(OpenCodeModelsEmpty));
    }

    /// <summary>
    /// Rebuilds <see cref="OpenCodeFilteredModels"/> from <see cref="OpenCodeModels"/> using the
    /// current <see cref="OpenCodeModelFilter"/> (case-insensitive <c>Contains</c>). The full list
    /// is shown when the filter is empty or when it just mirrors the committed selection (so the
    /// box can display the active model without narrowing the dropdown to a single entry); the
    /// list only narrows when the user is actively typing a partial query.
    /// </summary>
    private void RefreshOpenCodeFilteredModels()
    {
        var filter = OpenCodeModelFilter ?? string.Empty;
        bool isFullSelection = string.IsNullOrEmpty(filter)
            || string.Equals(filter, OpenCodeSelectedModel, StringComparison.Ordinal);
        IEnumerable<string> source = isFullSelection
            ? OpenCodeModels
            : OpenCodeModels.Where(m => m.Contains(filter, StringComparison.OrdinalIgnoreCase));

        OpenCodeFilteredModels = new ObservableCollection<string>(source);
    }

    /// <summary>
    /// When the filter text changes (the user is typing in the editable ComboBox), refresh the
    /// filtered dropdown so the list narrows live as they type.
    /// </summary>
    partial void OnOpenCodeModelFilterChanged(string value) => RefreshOpenCodeFilteredModels();

    /// <summary>
    /// When the full model list changes (serve connected / reconnected), refresh the filtered
    /// projection so the dropdown reflects the latest available models.
    /// </summary>
    partial void OnOpenCodeModelsChanged(ObservableCollection<string> value)
        => RefreshOpenCodeFilteredModels();

    /// <summary>
    /// Re-fetches the serve model list when serve connects while the panel is open, so a
    /// dropdown that opened disconnected populates the moment serve comes up. No-op when serve
    /// disconnects or the panel is closed.
    /// </summary>
    private void OnServeConnectionChanged(object? sender, bool isConnected)
    {
        if (!isConnected || !IsOpenCodePanelOpen) return;
        Avalonia.Threading.Dispatcher.UIThread.Post(async () => await LoadOpenCodeModelsFromServeAsync());
    }

    /// <summary>True when at least one model is available (drives the ComboBox IsEnabled).</summary>
    public bool OpenCodeHasModels => OpenCodeModels.Count > 0;

    /// <summary>True when no models are available — i.e. serve is down (drives the hint text).</summary>
    public bool OpenCodeModelsEmpty => OpenCodeModels.Count == 0;

    /// <summary>
    /// Loads the OpenCode template list from <c>settings/opencode/templates/</c> into
    /// <see cref="OpenCodeTemplates"/>. The <see cref="OpenCodeTemplate.None"/> sentinel
    /// is always first, keeping the selector optional. Safe to call before the panel
    /// opens; the service seeds and reads files without throwing.
    /// </summary>
    private async Task LoadOpenCodeTemplatesAsync()
    {
        var templates = await _openCodeTemplateService.LoadAsync();
        var collection = new ObservableCollection<OpenCodeTemplate> { OpenCodeTemplate.None };
        foreach (var template in templates)
            collection.Add(template);
        OpenCodeTemplates = collection;
    }

    /// <summary>
    /// Loads the OpenCode prompt list from <c>settings/opencode/prompts.json</c> into
    /// <see cref="OpenCodePrompts"/>. The <see cref="OpenCodePromptEntry.None"/> sentinel is
    /// always first, keeping the selector optional. Safe to call before the panel opens;
    /// the service seeds and reads the file without throwing.
    /// </summary>
    private async Task LoadOpenCodePromptsAsync()
    {
        var prompts = await _openCodePromptService.LoadAsync();
        var collection = new ObservableCollection<OpenCodePromptEntry> { OpenCodePromptEntry.None };
        foreach (var prompt in prompts)
            collection.Add(prompt);
        OpenCodePrompts = collection;
    }

    /// <summary>
    /// When a real prompt is selected, load its text into the Start prompt box (the user
    /// can still edit it before launching). The None sentinel leaves the prompt untouched.
    /// </summary>
    partial void OnOpenCodeSelectedPromptChanged(OpenCodePromptEntry value)
    {
        if (value is null || value.IsNone)
            return;
        OpenCodePrompt = value.Prompt;
    }

    private void OnRepoChanged(object? sender, EventArgs e)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            IsBusy = _repoService.IsBusy;
            RebuildTagFilters();
            ApplyFilter();
        });
    }

    partial void OnFilterTextChanged(string value) => ApplyFilter();

    /// <summary>
    /// Re-apply the filter when the panel closes so the OpenCode-targeted repo is not
    /// left visually selected.
    /// </summary>
    partial void OnIsOpenCodePanelOpenChanged(bool value)
    {
        if (!value)
        {
            OpenCodeRepo = null;
            OpenCodeInstanceCount = 1;
            ArrangeInGrid = false;
            OpenCodeSelectedModel = _defaultOpenCodeModel;
            OpenCodeSelectedTemplate = OpenCodeTemplate.None;
            OpenCodeSelectedPrompt = OpenCodePromptEntry.None;
            OpenCodePrompt = string.Empty;
            NewPromptName = string.Empty;
            OpenCodeAutoApprove = false;
        }
    }

    /// <summary>
    /// Auto-enable grid tiling as soon as more than one instance is requested; a single
    /// instance needs no positioning. The user can turn it back off manually afterwards
    /// (this only reacts to count changes, not manual toggles of the checkbox).
    /// </summary>
    partial void OnOpenCodeInstanceCountChanged(int value)
    {
        if (value > 1)
            ArrangeInGrid = true;
    }

    [RelayCommand]
    private void ClearFilter() => FilterText = string.Empty;

    /// <summary>
    /// Clears the search box and every tag checkbox (does not touch the tags on the
    /// repos themselves).
    /// </summary>
    [RelayCommand]
    private void ClearTagFilters()
    {
        FilterText = string.Empty;
        foreach (var tag in TagFilters)
            tag.IsChecked = false;
        ApplyFilter();
    }

    /// <summary>
    /// Called from the view when a tag checkbox is toggled, since TagFilter.IsChecked
    /// changes do not flow through this VM's own property-change pipeline.
    /// </summary>
    [RelayCommand]
    private void TagFilterChanged() => ApplyFilter();

    private void RebuildTagFilters()
    {
        var previous = TagFilters.ToDictionary(t => t.Name, t => t.IsChecked);
        var tags = _repoService.AllTags
            .OrderBy(t => t, StringComparer.OrdinalIgnoreCase)
            .ToList();

        var rebuilt = new ObservableCollection<TagFilter>();
        foreach (var name in tags)
        {
            var wasChecked = previous.TryGetValue(name, out var c) && c;
            rebuilt.Add(new TagFilter(name) { IsChecked = wasChecked });
        }

        TagFilters = rebuilt;
        AvailableTags = new ObservableCollection<string>(
            _repoService.AllTags
                .Where(t => !string.Equals(t, Repo.PlatformTag, StringComparison.OrdinalIgnoreCase))
                .OrderBy(t => t, StringComparer.OrdinalIgnoreCase));
    }

    private void ApplyFilter()
    {
        var filter = FilterText?.Trim();
        var repos = _repoService.Repos;
        var checkedTags = TagFilters
            .Where(t => t.IsChecked)
            .Select(t => t.Name)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);

        IEnumerable<Repo> result = repos;
        if (checkedTags.Count > 0)
        {
            // OR: a repo passes if it has ANY of the checked tags.
            result = result.Where(r => r.Tags.Any(t => checkedTags.Contains(t.Name)));
        }

        if (!string.IsNullOrWhiteSpace(filter))
        {
            result = result.Where(r =>
                r.Name?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true
                || r.FolderPath?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true
                || r.SolutionPath?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true);
        }

        // Favorites always float to the top, then alphabetical by name.
        FilteredRepos = new ObservableCollection<Repo>(
            result
                .OrderByDescending(r => r.IsFavorite)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase));
    }

    // --- Launch commands ---

    [RelayCommand]
    private void OpenVisualStudio(Repo? repo)
    {
        if (repo?.SolutionPath is null) return;
        _processLauncher.StartProcess(repo.SolutionPath);
    }

    [RelayCommand]
    private void OpenFolder(string? folderPath) => _processLauncher.StartProcess(folderPath);

    [RelayCommand]
    private async Task OpenWithVSCodeAsync(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = _reposSettings.VSCodeExecutable ?? "code";

        // Route through the DevTools service (named pipe). The service runs
        // non-elevated, so VS Code launches non-elevated even when Tools runs as admin.
        try
        {
            await _devToolsClient.SendProcessLaunchRequestAsync(exe, folderPath);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenWithVSCode: pipe launch failed, falling back to direct launch");
            _processLauncher.StartProcess(exe, folderPath, hidden: true);
        }
    }

    [RelayCommand]
    private void OpenWithTerminal(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = _reposSettings.TerminalExecutable ?? "wt";
        var args = TerminalArgumentFormatter.BuildArguments(exe, folderPath);
        _processLauncher.StartProcess(exe, args);
    }

    // --- OpenCode panel ---

    [RelayCommand]
    private async Task OpenOpenCodePanelAsync(Repo? repo)
    {
        if (repo is null || !IsOpenCodeEnabled) return;
        OpenCodeRepo = repo;
        OpenCodeInstanceCount = 1;
        ArrangeInGrid = false;
        OpenCodeSelectedModel = _defaultOpenCodeModel;
        OpenCodeSelectedTemplate = OpenCodeTemplate.None;
        OpenCodeSelectedPrompt = OpenCodePromptEntry.None;
        OpenCodePrompt = string.Empty;
        NewPromptName = string.Empty;
        // Default auto-approve from settings; an existing tracked instance overrides it so the
        // toggle reflects the live instance's current state when reopening the panel.
        var existing = repo.FolderPath is null ? null : _openCodeServeService.GetInstance(repo.FolderPath);
        OpenCodeAutoApprove = existing?.AutoApprove ?? _openCodeServeSettings.AutoApprove;
        IsOpenCodePanelOpen = true;

        // Fetch the live model list from serve now that the panel is open. If serve is down,
        // the dropdown stays empty (with its hint) and OnServeConnectionChanged will populate
        // it when serve connects.
        await LoadOpenCodeModelsFromServeAsync();
    }

    [RelayCommand]
    private void CloseOpenCodePanel() => IsOpenCodePanelOpen = false;

    [RelayCommand]
    private async Task LaunchOpenCodeAsync()
    {
        var repo = OpenCodeRepo;
        if (repo?.FolderPath is null) return;

        // Copy the selected template (if any) to <repo>/.opencode before launching so
        // opencode picks it up. Replaces an existing .opencode wholesale. No-op for None.
        await _openCodeTemplateService.CopyToRepoAsync(OpenCodeSelectedTemplate, repo.FolderPath);

        var terminalExe = _reposSettings.TerminalExecutable ?? "wt";
        var openCodeExe = _reposSettings.OpenCodeExecutable ?? "opencode";
        var prompt = OpenCodePrompt?.Trim();
        var count = OpenCodeInstanceCount < 1 ? 1 : OpenCodeInstanceCount;
        var model = string.IsNullOrWhiteSpace(OpenCodeSelectedModel)
            ? _defaultOpenCodeModel
            : OpenCodeSelectedModel;

        if (ArrangeInGrid)
        {
            // Tile the instances across the active screen: a single instance fills the
            // screen as a 1x1 grid, 6 instances form a 3x2 grid. The launcher owns window
            // detection and positioning, so single and multiple share one path/class.
            await _openCodeGridLauncher.LaunchAsync(terminalExe, openCodeExe, repo.FolderPath, model, prompt ?? string.Empty, count);
        }
        else
        {
            // Legacy path: open N terminals without positioning. The command line is
            // built with the same helper as the grid path so both stay in sync.
            var commandLine = OpenCodeGridLauncher.BuildCommandLine(openCodeExe, model, prompt ?? string.Empty);
            var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, commandLine);

            for (var i = 0; i < count; i++)
            {
                _processLauncher.StartProcess(terminalExe, args);
            }
        }

        // Register the launched instance with the serve service so it can be matched to a
        // serve session, tracked for status, and auto-approved (when enabled). The launcher is
        // fire-and-forget, so pid is unknown here; matching happens by working directory.
        _openCodeServeService.Register(repo.FolderPath, pid: null, OpenCodeAutoApprove);

        IsOpenCodePanelOpen = false;
    }

    /// <summary>
    /// When the panel's auto-approve toggle changes while an instance for the open repo is
    /// already tracked, propagate the new value to the live instance immediately.
    /// </summary>
    partial void OnOpenCodeAutoApproveChanged(bool value)
    {
        if (OpenCodeRepo?.FolderPath is not null)
            _openCodeServeService.SetAutoApprove(OpenCodeRepo.FolderPath, value);
    }

    /// <summary>
    /// Saves the current Start prompt under the name in <see cref="NewPromptName"/> (a
    /// blank prompt body is allowed — only the name is required). Reloads the selector and
    /// selects the saved entry so the panel reflects the persisted state.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanSavePrompt))]
    private async Task SavePromptAsync()
    {
        var name = (NewPromptName ?? string.Empty).Trim();
        if (string.IsNullOrEmpty(name)) return;

        await _openCodePromptService.SaveAsync(name, OpenCodePrompt ?? string.Empty);
        NewPromptName = string.Empty;

        await LoadOpenCodePromptsAsync();

        // Select the just-saved prompt so the UI reflects what was persisted.
        OpenCodeSelectedPrompt = OpenCodePrompts.FirstOrDefault(p =>
            string.Equals(p.Name, name, StringComparison.OrdinalIgnoreCase)) ?? OpenCodePromptEntry.None;
        _notificationService.Show("Prompt saved", NotificationKind.Success);
    }

    /// <summary>
    /// Save requires a non-empty name and some prompt text to be worth persisting.
    /// </summary>
    private bool CanSavePrompt()
        => !string.IsNullOrWhiteSpace(NewPromptName) && !string.IsNullOrWhiteSpace(OpenCodePrompt);

    /// <summary>Re-evaluate <see cref="SavePromptCommand"/> when its inputs change.</summary>
    partial void OnNewPromptNameChanged(string value) => SavePromptCommand.NotifyCanExecuteChanged();
    partial void OnOpenCodePromptChanged(string value) => SavePromptCommand.NotifyCanExecuteChanged();

    /// <summary>
    /// Removes the selected repo's <c>.opencode</c> folder and re-copies the currently
    /// selected template into it, without launching OpenCode. Useful to re-apply a
    /// template (and pick up edits to it) independently of launching.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanResetOpenCodeTemplate))]
    private async Task ResetOpenCodeTemplateAsync()
    {
        var repo = OpenCodeRepo;
        if (repo?.FolderPath is null || OpenCodeSelectedTemplate.IsNone)
            return;

        // CopyToRepoAsync deletes any existing .opencode first, then re-copies the
        // template folder — exactly the "remove and re-seed" behavior.
        await _openCodeTemplateService.CopyToRepoAsync(OpenCodeSelectedTemplate, repo.FolderPath);
        _notificationService.Show("Template reset", NotificationKind.Success);
    }

    /// <summary>Reset needs both a real template selection and a target repo.</summary>
    private bool CanResetOpenCodeTemplate()
        => OpenCodeRepo?.FolderPath is not null && !OpenCodeSelectedTemplate.IsNone;

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

    partial void OnOpenCodeRepoChanged(Repo? value)
    {
        ResetOpenCodeTemplateCommand.NotifyCanExecuteChanged();
        OnPropertyChanged(nameof(OpenCodeRepoName));
    }

    // --- Tag management ---

    [RelayCommand]
    private async Task AddTagAsync(Repo? repo)
    {
        if (repo is null) return;
        var tag = (NewTagText ?? string.Empty).Trim();
        NewTagText = string.Empty;
        if (string.IsNullOrEmpty(tag)) return;
        await _repoService.AddTagAsync(repo, tag);
    }

    /// <summary>
    /// Adds a specific tag name (e.g. from a quick-add chip in the flyout) to a repo.
    /// Parameter is a <see cref="Tuple{T1, T2}"/> of (Repo, tag-name).
    /// </summary>
    [RelayCommand]
    private async Task AddTagByNameAsync(Tuple<Repo, string>? repoAndTag)
    {
        if (repoAndTag is null) return;
        await _repoService.AddTagAsync(repoAndTag.Item1, repoAndTag.Item2);
    }

    [RelayCommand]
    private async Task RemoveTagAsync(RepoTag? repoTag)
    {
        if (repoTag is null) return;
        await _repoService.RemoveTagAsync(repoTag.Repo, repoTag.Name);
    }

    [RelayCommand]
    private Task ToggleFavoriteAsync(Repo? repo)
    {
        if (repo is null) return Task.CompletedTask;
        return _repoService.ToggleFavoriteAsync(repo);
    }

    // --- Settings & refresh ---

    [RelayCommand]
    private async Task RefreshAsync()
    {
        await _repoService.RefreshAsync(_reposSettings);
        // Re-check git statuses right away against the current list; the scan started
        // above raises Changed when it completes, which triggers one more pass.
        _ = _gitStatusService.RefreshAllAsync();
    }

    [RelayCommand]
    private async Task OpenSettingsAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var currentRepoSettings = settings.Repos ?? new ReposSettings();

            var edited = await _dialogService.ShowReposSettingsDialogAsync(currentRepoSettings);
            if (edited == null)
            {
                // User cancelled the dialog.
                return;
            }

            settings.Repos = edited;
            await _settingsService.SaveSettingsAsync(settings);

            _reposSettings = edited;
            await _repoService.RefreshAsync(_reposSettings);
            _notificationService.Show("Settings saved", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error opening repo settings");
            _notificationService.Show("Failed to save settings", NotificationKind.Error);
        }
    }
}
