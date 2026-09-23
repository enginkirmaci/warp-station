using System.IO;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Mvvm;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// The Yesterday's Summary tool (floating drawer, title-bar sparkle and tools
/// dropdown): one opencode-written markdown report covering ALL repositories with
/// activity on the previous local day — each one's commits plus its still-uncommitted
/// changes. Persisted once per day under the notes store's <c>Summaries</c> folder
/// (<c>&lt;store root&gt;/Summaries/&lt;yyyy-MM-dd&gt;.md</c>, so it also shows up in
/// the Notes page's whole-store view); opening loads the day's file when one exists
/// and generates it otherwise, the wand regenerates on demand.
/// </summary>
public partial class YesterdaySummaryViewModel : PageViewModelBase, IToolDrawerTeardown
{
    private readonly IRepoService _repoService;
    private readonly IGitStatusService _gitStatusService;
    private readonly ISettingsService _settingsService;
    private readonly INotesService _notes;
    private readonly INotificationService _notifications;
    private readonly YesterdaySummaryGenerator _generator;

    /// <summary>Notes subfolder the summaries live in, under the notes store root.</summary>
    private const string SummariesFolder = "Summaries";

    public YesterdaySummaryViewModel(
        IRepoService repoService,
        IGitStatusService gitStatusService,
        ISettingsService settingsService,
        INotesService notes,
        INotificationService notifications,
        IOpenCodeRunService openCodeRunService,
        IDailySummaryPromptService promptService)
    {
        _repoService = repoService;
        _gitStatusService = gitStatusService;
        _settingsService = settingsService;
        _notes = notes;
        _notifications = notifications;
        _generator = new YesterdaySummaryGenerator(openCodeRunService, promptService);
    }

    /// <summary>True while opencode writes the summary; drives the spinner.</summary>
    [ObservableProperty]
    private bool _isGenerating;

    /// <summary>The generated (or disk-loaded) markdown report; null when none yet.</summary>
    [ObservableProperty]
    private string? _summaryMarkdown;

    /// <summary>Header label — the day the report covers ("Monday, Sep 21, 2026").</summary>
    [ObservableProperty]
    private string? _summaryDateLabel;

    /// <summary>Nothing to report: no repository had commits or carries changes.</summary>
    [ObservableProperty]
    private bool _hasNoActivity;

    public bool HasSummary => !string.IsNullOrWhiteSpace(SummaryMarkdown);

    public bool ShowEmptyState => !HasSummary && !IsGenerating;

    public string EmptyStateText => HasNoActivity
        ? "No commits or changes were recorded yesterday across your repositories."
        : "No summary yet — press the wand to generate one.";

    /// <summary>The wand affordance: gated on the in-flight run only — the OpenCode
    /// enabled flag is re-checked inside the command (with a toast), so the button
    /// never goes stale on a settings flip.</summary>
    public bool CanGenerate => !IsGenerating;

    partial void OnIsGeneratingChanged(bool value)
    {
        OnPropertyChanged(nameof(ShowEmptyState));
        GenerateCommand.NotifyCanExecuteChanged();
    }

    partial void OnHasNoActivityChanged(bool value) => OnPropertyChanged(nameof(EmptyStateText));

    /// <summary>Drawer open: shows the day's file when one exists, generates otherwise
    /// (the sparkle's old auto-generate contract).</summary>
    public override Task OnNavigatedToAsync(object? parameter = null)
        => LoadOrPrepareAsync(generateWhenMissing: true);

    /// <summary>Cancels any in-flight generation (drawer close, app shutdown).</summary>
    public void OnDrawerClosed() => _generator.Cancel();

    [RelayCommand(CanExecute = nameof(CanGenerate))]
    private async Task GenerateAsync()
    {
        if (IsGenerating) return;
        await LoadOrPrepareAsync(generateWhenMissing: true, forceGenerate: true);
    }

    /// <summary>Shows the day's file when it exists, then generates when allowed.</summary>
    private async Task LoadOrPrepareAsync(bool generateWhenMissing, bool forceGenerate = false)
    {
        HasNoActivity = false;
        var day = Yesterday();
        SummaryDateLabel = YesterdaySummaryGenerator.FormatDayLabel(day);

        if (!forceGenerate)
        {
            var path = await ResolveSummaryPathAsync(day);
            if (path is not null && File.Exists(path))
            {
                try
                {
                    SetSummary(await _notes.ReadNoteAsync(path));
                    return;
                }
                catch (Exception ex)
                {
                    Log.Logger.Warning(ex, "Daily summary read failed for {Path}", path);
                }
            }

            if (!generateWhenMissing)
            {
                SetSummary(null);
                return;
            }
        }

        await GenerateCoreAsync(day);
    }

    /// <summary>
    /// Gathers every repository's day activity, runs the prompt through opencode once
    /// for the active ones, shows the answer and persists it into the notes store.
    /// </summary>
    private async Task GenerateCoreAsync(DateOnly day)
    {
        var settings = await ReadSettingsAsync();
        if (settings.OpenCode?.EnableOpenCode != true)
        {
            _notifications.Show("Enable the OpenCode integration to generate summaries", NotificationKind.Warning);
            return;
        }

        var token = _generator.Begin();
        IsGenerating = true;
        try
        {
            var activities = new List<RepoDayActivity>();
            foreach (var repo in _repoService.Repos)
            {
                token.ThrowIfCancellationRequested();
                var activity = await _gitStatusService.GetDayActivityAsync(repo, day, token);
                if (!activity.IsEmpty)
                {
                    activities.Add(new RepoDayActivity(repo.Name, activity));
                }
            }

            if (activities.Count == 0)
            {
                SetSummary(null);
                HasNoActivity = true;
                return;
            }

            var openCode = settings.OpenCode!;
            var model = !string.IsNullOrWhiteSpace(openCode.CommitModel) ? openCode.CommitModel!.Trim() : openCode.DefaultModel;
            var summary = await _generator.TryGenerateAsync(
                token, activities, SummaryDateLabel ?? YesterdaySummaryGenerator.FormatDayLabel(day),
                settings.Repos?.OpenCodeExecutable ?? ReposSettings.DefaultOpenCodeExecutable,
                model);
            if (summary is null)
            {
                _notifications.Show("Could not generate yesterday's summary", NotificationKind.Error);
                return;
            }

            SetSummary(summary);
            await SaveAsync(day, summary);
        }
        catch (OperationCanceledException)
        {
            // Drawer closed or app shutdown mid-run: the opencode child is already
            // dead, the card stays as-is — drop the run without an error toast.
        }
        finally
        {
            _generator.End();
            IsGenerating = false;
        }
    }

    private async Task SaveAsync(DateOnly day, string summary)
    {
        try
        {
            var path = await ResolveSummaryPathAsync(day);
            if (path is null) return;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            await _notes.WriteNoteAsync(path, summary);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Daily summary save failed");
        }
    }

    /// <summary>The day's file in the notes store: <c>&lt;store root&gt;/Summaries/&lt;yyyy-MM-dd&gt;.md</summary>
    private async Task<string?> ResolveSummaryPathAsync(DateOnly day)
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var storeRoot = _notes.ResolveStoreRoot(settings.General?.NotesStorePath);
            return Path.Combine(storeRoot, SummariesFolder, $"{day:yyyy-MM-dd}.md");
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Daily summary path resolution failed");
            return null;
        }
    }

    private async Task<AppSettings> ReadSettingsAsync()
    {
        try
        {
            return await _settingsService.GetSettingsAsync();
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Daily summary settings read failed");
            return new AppSettings();
        }
    }

    private void SetSummary(string? markdown)
    {
        SummaryMarkdown = markdown;
        OnPropertyChanged(nameof(HasSummary));
        OnPropertyChanged(nameof(ShowEmptyState));
    }

    /// <summary>The report's target: the previous local day.</summary>
    private static DateOnly Yesterday() => DateOnly.FromDateTime(DateTime.Today).AddDays(-1);
}
