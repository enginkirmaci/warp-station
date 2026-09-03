using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Formatters;
using Tools.Library.Mvvm;
using Tools.Library.Services;
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
    private readonly IGitHubService _gitHubService;
    private readonly IProcessLauncher _processLauncher;
    private readonly IOpenCodeTemplateService _openCodeTemplateService;
    private readonly IOpenCodePromptService _openCodePromptService;
    private readonly IOpenCodeGridLauncher _openCodeGridLauncher;
    private readonly IOpenCodeModelService _openCodeModelService;
    private readonly INotificationService _notificationService;
    private ReposSettings _reposSettings = new();
    private OpenCodeSettings _openCodeSettings = new();

    /// <summary>
    /// Debounce timers for the filter and the service-changed handler. A burst of typing or
    /// the several <c>Changed</c> raises a single scan produces each cancel the pending
    /// callback and restart the window, so only one in-place <see cref="ApplyFilter"/> runs
    /// per burst instead of tearing down the list per keystroke / per event.
    /// </summary>
    private CancellationTokenSource? _filterDebounce;
    private CancellationTokenSource? _changedDebounce;

    /// <summary>Idle window for the search-box filter before the list is re-synced.</summary>
    private const int FilterDebounceMs = 150;

    /// <summary>
    /// Idle window for coalescing the multiple <c>Changed</c> raises a single scan emits
    /// (start, data-ready, finally) into one rebuild.
    /// </summary>
    private const int ChangedDebounceMs = 100;

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

    /// <summary>
    /// Tracks an in-flight refresh (repo scan + git status pass) so only the Refresh
    /// button reflects it — the rest of the page (search, tags, cards, OpenCode panel,
    /// and the per-card "checking…" git placeholders) stays interactive throughout.
    /// Kept separate from the base <see cref="ViewModelBase.IsBusy"/> (which mirrors the
    /// repo service's scan state) so nothing else on the page is gated by a refresh.
    /// </summary>
    [ObservableProperty]
    private bool _isRefreshing;

    partial void OnIsRefreshingChanged(bool value) => RefreshCommand.NotifyCanExecuteChanged();

    // --- OpenCode panel (transient state) ---

    /// <summary>
    /// Whether the OpenCode integration is enabled (mirrors <see cref="OpenCodeSettings.Enabled"/>).
    /// When false, the launch panel cannot open and all per-repo OpenCode UI is hidden.
    /// </summary>
    [ObservableProperty]
    private bool _isOpenCodeEnabled;

    // --- GitHub column visibility ---

    /// <summary>
    /// Whether the GitHub column shows (mirrors <see cref="ReposSettings.ShowGitHubColumn"/>).
    /// When false the whole column cell collapses — and the GitHub service is configured
    /// off too, so no <c>gh</c> processes are spawned for a column that is not visible.
    /// </summary>
    [ObservableProperty]
    private bool _isGitHubColumnVisible;

    // --- Launch shortcut availability (per PC) ---

    /// <summary>
    /// The launch buttons are only shown when their executable actually resolves on this
    /// machine (see <see cref="ExecutableDefaults"/>): a button whose target is missing
    /// would spawn a terminal "command not found" or nothing at all, so it is hidden
    /// instead. Recomputed whenever settings load or are saved —
    /// <see cref="RefreshShortcutAvailability"/>. Windows keeps the configured name
    /// verbatim (CreateProcess resolves it), so there a configured executable is always
    /// considered available and the buttons keep the pre-availability behavior.
    /// </summary>
    [ObservableProperty]
    private bool _hasTerminal;

    /// <summary>Whether the open-solution action can run. It is a Visual Studio action, so
    /// it needs Visual Studio installed; a non-empty <see cref="ReposSettings.IdeExecutable"/>
    /// overrides the check (Locate-verified on Linux) so e.g. Rider can be wired up
    /// deliberately. Windows without VS and the .sln association re-pointed elsewhere
    /// hides the button until an IDE is configured.</summary>
    [ObservableProperty]
    private bool _hasIde;

    [ObservableProperty]
    private bool _hasVSCode;

    [ObservableProperty]
    private bool _hasZCode;

    /// <summary>
    /// Whether the per-repo OpenCode buttons show: the integration must be enabled in
    /// settings <em>and</em> the configured opencode CLI must resolve on this machine.
    /// </summary>
    [ObservableProperty]
    private bool _hasOpenCode;

    /// <summary>
    /// Re-evaluates the per-PC launch-shortcut availability flags from the current
    /// settings. Called after settings load and after the settings dialog saves. The
    /// underlying probes are memoized per process (see <see cref="ExecutableDefaults"/>),
    /// so the filesystem/vswhere work happens once per executable — repeat calls only
    /// re-run when a configured value actually changed.
    /// </summary>
    private void RefreshShortcutAvailability()
    {
        HasTerminal = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable) is not null;

        // The open-solution button is a Visual Studio shortcut: only VS itself (or an
        // explicitly configured stand-in IDE) makes it available — a detected editor like
        // VS Code must not light it up, it has its own button.
        var configuredIde = _reposSettings.IdeExecutable?.Trim();
        HasIde = !string.IsNullOrEmpty(configuredIde)
            ? ExecutableDefaults.Locate(configuredIde) is not null
            : ExecutableDefaults.HasVisualStudio();

        HasVSCode = ExecutableDefaults.Locate(_reposSettings.VSCodeExecutable) is not null;
        HasZCode = ExecutableDefaults.Locate(_reposSettings.ZCodeExecutable) is not null;
        HasOpenCode = IsOpenCodeEnabled && ExecutableDefaults.Locate(_reposSettings.OpenCodeExecutable) is not null;
    }

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
    /// Whether to tile the launched opencode instances across the screen in a grid. Off by
    /// default — instances open as plain terminal windows; checking it routes the launch
    /// through <see cref="IOpenCodeGridLauncher"/>, which positions each window into a cell.
    /// Reset to off each time the panel closes, like the other panel fields.
    /// </summary>
    [ObservableProperty]
    private bool _openCodeArrangeIntoGrid;

    /// <summary>
    /// Whether the OpenCode panel offers the "Arrange into grid" checkbox. The grid launcher
    /// positions windows through SnapIt's Win32 <see cref="IWinApiService"/>, so the option
    /// only exists on Windows; the panel hides it elsewhere. Constant per process, so no
    /// change notification is needed.
    /// </summary>
    public bool CanArrangeIntoGrid => OperatingSystem.IsWindows();

    /// <summary>
    /// The models available in the OpenCode model selector, fetched by running
    /// <c>opencode models</c> as a one-shot process (see <see cref="IOpenCodeModelService"/>).
    /// (Re)populated each time the panel opens; empty when the CLI fails or is missing.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<string> _openCodeModels = new();

    /// <summary>
    /// The currently selected model in the OpenCode panel. Passed to opencode via
    /// <c>opencode model "&lt;model&gt;"</c>. Bound OneWay to the sidebar's model picker so the
    /// box genuinely selects (highlights) the configured default model; user picks are
    /// committed by the page code-behind's SelectionChanged handler, not by a TwoWay
    /// binding — a TwoWay writeback would null the selection during the in-place list
    /// rebuilds (see <see cref="RefreshOpenCodeFilteredModels"/>) and break the
    /// pick-survives-refresh behavior.
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
        IGitHubService gitHubService,
        IProcessLauncher processLauncher,
        IOpenCodeTemplateService openCodeTemplateService,
        IOpenCodePromptService openCodePromptService,
        IOpenCodeGridLauncher openCodeGridLauncher,
        IOpenCodeModelService openCodeModelService,
        INotificationService notificationService)
    {
        _settingsService = settingsService;
        _devToolsClient = devToolsClient;
        _dialogService = dialogService;
        _repoService = repoService;
        _gitStatusService = gitStatusService;
        _gitHubService = gitHubService;
        _processLauncher = processLauncher;
        _openCodeTemplateService = openCodeTemplateService;
        _openCodePromptService = openCodePromptService;
        _openCodeGridLauncher = openCodeGridLauncher;
        _openCodeModelService = openCodeModelService;
        _notificationService = notificationService;

        _repoService.Changed += OnRepoChanged;
        _repoService.TagsChanged += OnRepoChanged;
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override Task OnNavigatedFromAsync()
    {
        // Detach from the singletons so this Transient VM (rebuilt per navigation) is not
        // kept alive by them and does not receive further state changes.
        _repoService.Changed -= OnRepoChanged;
        _repoService.TagsChanged -= OnRepoChanged;

        // Cancel any deferred filter/changed callbacks so a pending debounce does not fire
        // its UI-thread update after this VM is no longer the active page.
        _filterDebounce?.Cancel();
        _filterDebounce?.Dispose();
        _changedDebounce?.Cancel();
        _changedDebounce?.Dispose();
        _filterDebounce = null;
        _changedDebounce = null;
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        _reposSettings = settings.Repos ?? new ReposSettings();
        _openCodeSettings = settings.OpenCode ?? new OpenCodeSettings();
        IsOpenCodeEnabled = _openCodeSettings.Enabled;
        IsGitHubColumnVisible = _reposSettings.ShowGitHubColumn;
        // Configure the GitHub service before loading repos: both the explicit kick below
        // and the service's own scan-triggered refresh gate on this flag, so a disabled
        // column never spawns gh even during the initial scan burst.
        _gitHubService.Configure(_reposSettings);
        RefreshShortcutAvailability();
        await _repoService.EnsureLoadedAsync(_reposSettings);
        await LoadOpenCodeTemplatesAsync();
        await LoadOpenCodePromptsAsync();
        RebuildTagFilters();
        ApplyFilter();

        // Kick the local git status checks in the background — the cards render instantly
        // with a "checking…" placeholder and the counts fill in as each repo's probe
        // completes. Only repos without a status yet need probing (first navigation, or
        // after a scan added repos); later navigations of the same session reuse the
        // statuses the earlier passes pushed onto the entities, instead of re-spawning
        // one git process per repo on every page visit.
        if (_repoService.Repos.Any(r => !r.GitStatusLoaded))
        {
            _ = _gitStatusService.RefreshAllAsync();
        }

        // Same lazy pattern for the GitHub column: only probe when the column is visible
        // and some repo has no GitHub data yet (first navigation or after new repos);
        // later navigations reuse the counts already pushed onto the entities.
        if (IsGitHubColumnVisible && _repoService.Repos.Any(r => !r.GitHubLoaded))
        {
            _ = _gitHubService.RefreshAllAsync();
        }
    }

    /// <summary>
    /// Loads the OpenCode model list: the cached list from the last successful run is shown
    /// immediately so the selector is usable right away, then <c>opencode models</c> runs (see
    /// <see cref="IOpenCodeModelService"/>) and the fresh list replaces it. A fresh empty result
    /// (CLI unavailable) clears the selector, which shows its "no models" hint. Called each time
    /// the panel opens.
    /// </summary>
    private async Task LoadOpenCodeModelsAsync()
    {
        var cached = _openCodeModelService.GetCachedModels(_openCodeSettings.DefaultModel);
        if (cached.Count > 0)
            ApplyOpenCodeModels(cached);

        var models = await _openCodeModelService.GetModelsAsync(_reposSettings.OpenCodeExecutable, _openCodeSettings.DefaultModel);
        ApplyOpenCodeModels(models);
    }

    /// <summary>
    /// Pushes <paramref name="models"/> into <see cref="OpenCodeModels"/>, selects the
    /// configured default or first entry (or clears the selection when empty), and refreshes
    /// the filter projection and the computed has/empty flags. A model the user already
    /// picked survives the refresh when it is still present: this runs again when the
    /// background CLI call finishes after the panel is already open, and resetting then
    /// would silently swap the user's pick for the default before launch.
    /// </summary>
    private void ApplyOpenCodeModels(IReadOnlyList<string> models)
    {
        OpenCodeModels = new ObservableCollection<string>(models);

        // Keep the committed selection when the refreshed list still contains it; otherwise
        // select the configured default (or the first model) on load, or clear the selection
        // when nothing is available.
        var previous = OpenCodeSelectedModel;
        var previousStillListed = !string.IsNullOrWhiteSpace(previous) && OpenCodeModels.Contains(previous);
        OpenCodeSelectedModel = previousStillListed
            ? previous
            : SelectConfiguredOrDefaultModel(OpenCodeModels);

        // The editable ComboBox binds its text to the filter and its dropdown to the filtered
        // list; mirror the committed selection into both so the box shows the active model.
        OpenCodeModelFilter = OpenCodeSelectedModel;
        RefreshOpenCodeFilteredModels();

        // The picker's SelectedItem is bound OneWay to the committed selection so the
        // dropdown genuinely highlights the selected (default) model. Re-raise it here so
        // the binding re-resolves after the in-place list rebuild above — including when
        // the value did not change and ObservableProperty raised nothing. Safe from text
        // clobbering: the filter was just mirrored to the same value, so the selection's
        // text equals what the box already shows, and the code-behind's SelectionChanged
        // handler re-commits equal values (no property-change, no loop).
        OnPropertyChanged(nameof(OpenCodeSelectedModel));

        // The computed has/empty flags feed the ComboBox IsEnabled and the hint visibility.
        OnPropertyChanged(nameof(OpenCodeHasModels));
        OnPropertyChanged(nameof(OpenCodeModelsEmpty));
    }

    /// <summary>
    /// The model to preselect (and launch) when the user has not picked one: the configured
    /// default (<see cref="OpenCodeSettings.DefaultModel"/>) when set and listed — matched
    /// case-insensitively and resolved to the list's own casing — otherwise the first model.
    /// The model service guarantees a configured default is present in the list, so this
    /// returns the default whenever one is configured.
    /// </summary>
    private string SelectConfiguredOrDefaultModel(IReadOnlyList<string> models)
    {
        var configured = _openCodeSettings.DefaultModel?.Trim();
        if (!string.IsNullOrEmpty(configured))
        {
            var match = models.FirstOrDefault(m => string.Equals(m, configured, StringComparison.OrdinalIgnoreCase));
            if (match is not null)
                return match;
        }

        return models.FirstOrDefault() ?? string.Empty;
    }

    /// <summary>
    /// The model to launch with, in priority order: an exact match for what the box shows
    /// (the user typed the full model id without committing a dropdown pick), the committed
    /// dropdown selection, and finally the configured default or the first model
    /// (see <see cref="SelectConfiguredOrDefaultModel"/>).
    /// </summary>
    private string ResolveOpenCodeLaunchModel()
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
    /// Rebuilds <see cref="OpenCodeFilteredModels"/> from <see cref="OpenCodeModels"/> using the
    /// current <see cref="OpenCodeModelFilter"/> (case-insensitive <c>Contains</c>). The full list
    /// is shown when the filter is empty or when it just mirrors the committed selection (so the
    /// box can display the active model without narrowing the dropdown to a single entry); the
    /// list only narrows when the user is actively typing a partial query.
    /// Must not run synchronously from a filter writeback that originates inside the ComboBox's
    /// own selection update — see <see cref="ScheduleFilteredModelsRefresh"/>.
    /// </summary>
    private void RefreshOpenCodeFilteredModels()
    {
        var filter = OpenCodeModelFilter ?? string.Empty;
        bool isFullSelection = string.IsNullOrEmpty(filter)
            || string.Equals(filter, OpenCodeSelectedModel, StringComparison.Ordinal);
        var source = (isFullSelection
            ? OpenCodeModels
            : OpenCodeModels.Where(m => m.Contains(filter, StringComparison.OrdinalIgnoreCase))).ToList();

        // Rebuild the existing collection in place rather than swapping in a new instance: the
        // ComboBox's Text binding raises the filter change from inside the control's own
        // selection update, and re-sourcing ItemsSource there throws "Cannot change source
        // while update is in progress". In-place collection-change notifications are safe —
        // the selection model batches them — and avoid a full ItemsSource reset per keystroke.
        //
        // Skip the rebuild entirely when the projection already matches: Clear() raises a
        // Reset on the picker's ItemsSource, which drops the ComboBox's control-side selection
        // even when the content is identical — the default the panel just preselected would
        // visibly deselect again on the follow-up deferred pass.
        if (source.Count == OpenCodeFilteredModels.Count && source.SequenceEqual(OpenCodeFilteredModels))
            return;

        OpenCodeFilteredModels.Clear();
        foreach (var model in source)
            OpenCodeFilteredModels.Add(model);
    }

    /// <summary>
    /// Whether a deferred <see cref="RefreshOpenCodeFilteredModels"/> pass is already queued
    /// (see <see cref="ScheduleFilteredModelsRefresh"/>).
    /// </summary>
    private bool _filteredModelsRefreshScheduled;

    /// <summary>
    /// When the filter text changes (the user is typing in the editable ComboBox), refresh the
    /// filtered dropdown so the list narrows live as they type — deferred to the dispatcher,
    /// never synchronous.
    /// </summary>
    partial void OnOpenCodeModelFilterChanged(string value) => ScheduleFilteredModelsRefresh();

    /// <summary>
    /// Schedules <see cref="RefreshOpenCodeFilteredModels"/> on the next dispatcher pass,
    /// coalescing bursts into one rebuild.
    /// <para>
    /// The deferral is load-bearing: the picker's Text binding writes back into
    /// <see cref="OpenCodeModelFilter"/> from inside the ComboBox's own selection update —
    /// setting <c>SelectedItem</c> (the OneWay binding that selects the configured default)
    /// updates the editable Text, and that Text change re-enters the VM synchronously.
    /// Mutating <see cref="OpenCodeFilteredModels"/> in that window raises CollectionChanged
    /// re-entrantly and the selection model throws "Source collection was modified during
    /// selection update". Deferring lets the selection update finish; the rebuild reads the
    /// current filter when it runs, so intermediate invocations coalesce safely.
    /// </para>
    /// </summary>
    private void ScheduleFilteredModelsRefresh()
    {
        if (_filteredModelsRefreshScheduled)
        {
            return;
        }

        _filteredModelsRefreshScheduled = true;
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            _filteredModelsRefreshScheduled = false;

            // Capture whether the box is supposed to be showing the committed selection (the
            // load/preselect paths mirror the filter onto it) before rebuilding — while a user
            // search is in flight the filter differs from the selection.
            bool boxShowsSelection = string.Equals(OpenCodeModelFilter, OpenCodeSelectedModel, StringComparison.Ordinal);
            RefreshOpenCodeFilteredModels();

            // A rebuild that actually runs drops the ComboBox's control-side selection (the
            // Clear() Reset clears it, and re-adding the items does not restore it). When the
            // box was showing the committed selection, re-push it: re-raising makes the OneWay
            // SelectedItem binding re-resolve and reselect the entry (and restore the box
            // text). During user typing the capture was false, so the search text is never
            // clobbered. The push changes no VM state that schedules another pass, so it
            // cannot loop.
            if (boxShowsSelection && !string.IsNullOrEmpty(OpenCodeSelectedModel))
                OnPropertyChanged(nameof(OpenCodeSelectedModel));
        });
    }

    /// <summary>
    /// When the full model list changes (panel reopened), refresh the filtered
    /// projection so the dropdown reflects the latest available models. Deferred like the
    /// filter path so a list swap can never mutate the source mid-selection-update.
    /// </summary>
    partial void OnOpenCodeModelsChanged(ObservableCollection<string> value)
        => ScheduleFilteredModelsRefresh();

    /// <summary>True when at least one model is available (drives the ComboBox IsEnabled).</summary>
    public bool OpenCodeHasModels => OpenCodeModels.Count > 0;

    /// <summary>True when no models are available — i.e. the CLI failed (drives the hint text).</summary>
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
        // Wired to both Changed and TagsChanged: both mean "the projection may be stale"
        // (fresh scan data, or a tag/favorite edit that can reorder or re-filter).
        //
        // Note: the repo service's scan state is intentionally NOT mirrored onto the
        // base IsBusy here — only IsRefreshing gates the Refresh button, so a scan never
        // blocks the rest of the page. The cards/tags/list re-render from the service
        // snapshot below without disabling anything.
        //
        // A single scan raises Changed several times (start, after replacing the repos,
        // and in the finally block). Debounce so those collapse into one rebuild pass
        // rather than tearing the list down and rebuilding it per event.
        ScheduleChangedDebounce();
    }

    /// <summary>
    /// Coalesces a burst of <see cref="IRepoService.Changed"/> raises into a single
    /// tag-filter rebuild + in-place list sync on the UI thread.
    /// </summary>
    private void ScheduleChangedDebounce()
    {
        _changedDebounce?.Cancel();
        _changedDebounce?.Dispose();
        _changedDebounce = new CancellationTokenSource();
        var token = _changedDebounce.Token;
        Task.Run(async () =>
        {
            try
            {
                await Task.Delay(ChangedDebounceMs, token);
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    if (token.IsCancellationRequested) return;
                    RebuildTagFilters();
                    ApplyFilter();
                });
            }
            catch (OperationCanceledException) { }
        });
    }

    partial void OnFilterTextChanged(string value) => ScheduleFilterDebounce();

    /// <summary>
    /// Coalesces a burst of keystrokes into a single in-place list sync so the cards are
    /// not torn down and rebuilt per character.
    /// </summary>
    private void ScheduleFilterDebounce()
    {
        _filterDebounce?.Cancel();
        _filterDebounce?.Dispose();
        _filterDebounce = new CancellationTokenSource();
        var token = _filterDebounce.Token;
        Task.Run(async () =>
        {
            try
            {
                await Task.Delay(FilterDebounceMs, token);
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    if (token.IsCancellationRequested) return;
                    ApplyFilter();
                });
            }
            catch (OperationCanceledException) { }
        });
    }

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
            OpenCodeArrangeIntoGrid = false;
            OpenCodeSelectedModel = string.Empty;
            OpenCodeModelFilter = string.Empty;
            OpenCodeSelectedTemplate = OpenCodeTemplate.None;
            OpenCodeSelectedPrompt = OpenCodePromptEntry.None;
            OpenCodePrompt = string.Empty;
            NewPromptName = string.Empty;
        }
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
        var ordered = result
            .OrderByDescending(r => r.IsFavorite)
            .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase)
            .ToList();

        // Skip the sync when the projection is unchanged (e.g. adding a tag while no tag
        // filter is checked, or a search term that matches the same set): Clear/Add would
        // churn every recycled container and re-render the list for nothing. Same-count
        // lists are compared by reference — repos are shared instances, and Repo has no
        // value-equality that would catch a name/path edit anyway.
        if (ordered.Count == FilteredRepos.Count)
        {
            var unchanged = true;
            for (var i = 0; i < ordered.Count; i++)
            {
                if (ReferenceEquals(ordered[i], FilteredRepos[i])) continue;
                unchanged = false;
                break;
            }

            if (unchanged) return;
        }

        // Sync the existing collection in place rather than replacing it. Reassigning a new
        // instance here would force every card container to be torn down and rebuilt (and
        // with a non-virtualizing panel, re-realized up front). Clear/Add flow through
        // CollectionChanged so the virtualized ListBox only recycles affected containers, and
        // the count binding ({Binding FilteredRepos.Count}) updates from those same notifications.
        FilteredRepos.Clear();
        foreach (var repo in ordered)
            FilteredRepos.Add(repo);
    }

    // --- Launch commands ---

    [RelayCommand]
    private void OpenVisualStudio(Repo? repo)
    {
        if (repo?.SolutionPath is null) return;

        // Windows keeps the .sln shell association unless an IDE is configured; other
        // platforms open the solution in an auto-detected .NET IDE (e.g. Rider).
        var ide = ExecutableDefaults.ResolveIde(_reposSettings.IdeExecutable);
        if (ide is null)
        {
            if (OperatingSystem.IsWindows())
            {
                _processLauncher.StartProcess(repo.SolutionPath);
            }
            else
            {
                Log.Logger.Warning("OpenVisualStudio: no .NET IDE found; configure one in Repos settings");
            }
            return;
        }

        _processLauncher.StartProcess(ide, $"\"{repo.SolutionPath}\"", stripElectronEnvironment: true);
    }

    [RelayCommand]
    private void OpenFolder(string? folderPath) => _processLauncher.StartProcess(folderPath);

    [RelayCommand]
    private async Task OpenWithVSCodeAsync(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        // Resolve early: on Linux the GUI PATH can miss user-level installs, so the
        // fallback launch below needs the absolute path, not the bare name.
        var exe = ExecutableDefaults.Locate(_reposSettings.VSCodeExecutable)
                  ?? _reposSettings.VSCodeExecutable
                  ?? "code";

        // When a profile is configured, launch VS Code with it (--profile <name>);
        // otherwise open with the default profile (no extra arguments).
        var args = string.IsNullOrWhiteSpace(_reposSettings.VSCodeProfile)
            ? folderPath
            : $"--profile \"{_reposSettings.VSCodeProfile}\" \"{folderPath}\"";

        // Route through the DevTools service (named pipe). The service runs
        // non-elevated, so VS Code launches non-elevated even when Tools runs as admin.
        try
        {
            await _devToolsClient.SendProcessLaunchRequestAsync(exe, args);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenWithVSCode: pipe launch failed, falling back to direct launch");
            _processLauncher.StartProcess(exe, args, hidden: true, stripElectronEnvironment: true);
        }
    }

    [RelayCommand]
    private void OpenWithTerminal(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (exe is null) return;

        var args = TerminalArgumentFormatter.BuildArguments(exe, folderPath);
        _processLauncher.StartProcess(exe, args, stripElectronEnvironment: true);
    }

    // --- GitHub column ---

    /// <summary>
    /// Opens the repo's GitHub page in the browser. A no-op for repos without a GitHub
    /// remote (their column cell shows nothing anyway).
    /// </summary>
    [RelayCommand]
    private void OpenGitHubRepo(Repo? repo)
    {
        if (string.IsNullOrWhiteSpace(repo?.GitHubRepoUrl)) return;
        _processLauncher.StartProcess(repo.GitHubRepoUrl);
    }

    /// <summary>
    /// Opens the GitHub details dialog for the repo: open pull requests first, then
    /// issues, as clickable links with a Refresh button. The dialog fetches fresh data
    /// from <see cref="IGitHubService"/> on open (seeding from its cache), independent
    /// of the column's visibility setting.
    /// </summary>
    [RelayCommand]
    private async Task OpenGitHubDetailsAsync(Repo? repo)
    {
        if (repo is null) return;
        await _dialogService.ShowGitHubDetailsDialogAsync(repo);
    }

    /// <summary>
    /// Opens zcode on the repo folder. The zcode AppImage is the Electron desktop package
    /// (it contains no interactive CLI runtime), so it is launched directly on the folder
    /// like VS Code — no terminal wrapper. A standalone zcode CLI binary has no UI of its
    /// own, so that variant still runs inside the configured terminal.
    /// </summary>
    [RelayCommand]
    private void OpenWithZCode(Repo? repo)
    {
        if (repo?.FolderPath is null) return;

        var resolved = ExecutableDefaults.Locate(_reposSettings.ZCodeExecutable)
                       ?? _reposSettings.ZCodeExecutable
                       ?? "zcode";

        if (!OperatingSystem.IsWindows() && resolved.EndsWith(".appimage", StringComparison.OrdinalIgnoreCase))
        {
            _processLauncher.StartProcess(resolved, $"\"{repo.FolderPath}\"", stripElectronEnvironment: true);
            return;
        }

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null) return;

        var zcodeExe = resolved.Contains(' ') ? $"\"{resolved}\"" : resolved;
        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, zcodeExe);
        _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
    }

    /// <summary>
    /// Resolves a CLI name for embedding in a terminal command line. The spawned
    /// terminal inherits the app's often-minimal GUI PATH, so a bare name is expanded
    /// to its absolute path; when unresolvable the bare name is kept so the terminal
    /// shows the familiar "command not found" feedback.
    /// </summary>
    private string ResolveCliForTerminal(string? configured, string fallback)
    {
        var resolved = ExecutableDefaults.Locate(configured) ?? configured ?? fallback;
        return resolved.Contains(' ') ? $"\"{resolved}\"" : resolved;
    }

    // --- OpenCode panel ---

    /// <summary>
    /// Quick open: launches a single opencode instance in the repo folder with the configured
    /// default model (or the first model from the list when none is configured) — no panel,
    /// no template/prompt/instance options. The cached list answers instantly; on a cold start
    /// the CLI runs once and fills the cache.
    /// </summary>
    [RelayCommand]
    private async Task QuickOpenOpenCodeAsync(Repo? repo)
    {
        if (repo?.FolderPath is null || !IsOpenCodeEnabled) return;

        var models = _openCodeModelService.GetCachedModels(_openCodeSettings.DefaultModel);
        if (models.Count == 0)
            models = await _openCodeModelService.GetModelsAsync(_reposSettings.OpenCodeExecutable, _openCodeSettings.DefaultModel);

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null) return;

        var openCodeExe = ResolveCliForTerminal(_reposSettings.OpenCodeExecutable, "opencode");
        var commandLine = OpenCodeGridLauncher.BuildCommandLine(openCodeExe, SelectConfiguredOrDefaultModel(models), string.Empty);
        var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, commandLine);
        _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
    }

    [RelayCommand]
    private void OpenOpenCodePanel(Repo? repo)
    {
        if (repo is null || !IsOpenCodeEnabled) return;
        OpenCodeRepo = repo;
        OpenCodeInstanceCount = 1;
        OpenCodeSelectedTemplate = OpenCodeTemplate.None;
        OpenCodeSelectedPrompt = OpenCodePromptEntry.None;
        OpenCodePrompt = string.Empty;
        NewPromptName = string.Empty;
        IsOpenCodePanelOpen = true;

        // Load the model list in the background so this command returns immediately — the
        // cached list is shown right away and 'opencode models' (a multi-second CLI call)
        // refreshes it when it returns. Awaiting it would keep this async command "running"
        // (CommunityToolkit's AsyncRelayCommand is non-concurrent, so CanExecute stays false),
        // which leaves the Options button disabled for the whole CLI duration — even after the
        // panel is closed. Fire-and-forget keeps the button instantly re-clickable.
        _ = LoadOpenCodeModelsAsync();
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

        var terminalExe = ExecutableDefaults.ResolveTerminal(_reposSettings.TerminalExecutable);
        if (terminalExe is null)
        {
            IsOpenCodePanelOpen = false;
            return;
        }

        var openCodeExe = ResolveCliForTerminal(_reposSettings.OpenCodeExecutable, "opencode");
        var prompt = OpenCodePrompt?.Trim();
        var count = OpenCodeInstanceCount < 1 ? 1 : OpenCodeInstanceCount;
        var model = ResolveOpenCodeLaunchModel();

        if (OpenCodeArrangeIntoGrid)
        {
            // Tile the instances across the active screen: a single instance fills the
            // screen as a 1x1 grid, 6 instances form a 3x2 grid. The launcher owns window
            // detection and positioning, so single and multiple share one path/class.
            await _openCodeGridLauncher.LaunchAsync(terminalExe, openCodeExe, repo.FolderPath, model, prompt ?? string.Empty, count);
        }
        else
        {
            // No tiling: open the instances as plain terminal windows, the same way Quick
            // Open launches a single one. BuildCommandLine is shared with the grid path so
            // the command line stays in sync.
            var commandLine = OpenCodeGridLauncher.BuildCommandLine(openCodeExe, model, prompt ?? string.Empty);
            var args = TerminalArgumentFormatter.BuildCommandArguments(terminalExe, repo.FolderPath, commandLine);
            for (var i = 0; i < count; i++)
            {
                _processLauncher.StartProcess(terminalExe, args, stripElectronEnvironment: true);
            }
        }

        IsOpenCodePanelOpen = false;
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

    /// <summary>
    /// Re-scans the configured folders and re-checks every repo's git status. Only the
    /// Refresh button is disabled for the duration (see <see cref="IsRefreshing"/>); the
    /// rest of the page remains fully interactive. Re-entrant-safe: a refresh already in
    /// progress ignores further clicks via <see cref="CanRefresh"/>.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanRefresh))]
    private async Task RefreshAsync()
    {
        IsRefreshing = true;
        try
        {
            await _repoService.RefreshAsync(_reposSettings);
            // Re-check git statuses against the freshly scanned list and await them so the
            // button's busy state spans the whole cycle (scan + status). The scan itself
            // raises Changed on completion, which triggers one more status pass; awaiting
            // here coalesces both into a single IsRefreshing window.
            await _gitStatusService.RefreshAllAsync();
        }
        finally
        {
            IsRefreshing = false;
        }
    }

    /// <summary>A refresh can start only when one isn't already running.</summary>
    private bool CanRefresh() => !IsRefreshing;

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
            IsGitHubColumnVisible = edited.ShowGitHubColumn;
            _gitHubService.Configure(edited);
            RefreshShortcutAvailability();
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
