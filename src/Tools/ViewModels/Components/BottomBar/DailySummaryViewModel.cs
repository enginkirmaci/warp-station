using System.IO;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// The Overview sidebar's "yesterday" card (the title-bar sparkle button): an
/// opencode-written markdown report of the selected repo's previous local day — its
/// commits plus the still-uncommitted changes — persisted next to the repo's notes
/// (<c>&lt;notes store&gt;/&lt;repo&gt;/Summaries/&lt;yyyy-MM-dd&gt;.md</c>, so it also
/// shows up in the Notes page). Opening loads the day's file when one exists and
/// generates it otherwise; the wand regenerates on demand. Singleton lifetime via the
/// shell, like the bar's other panels.
/// </summary>
public partial class DailySummaryViewModel : ObservableObject
{
    private readonly BottomBarViewModel _shell;
    private readonly DailySummaryGenerator _generator;
    private readonly ISettingsService _settingsService;
    private readonly INotesService _notes;

    /// <summary>Notes subfolder the summaries live in, under the repo's notes root.</summary>
    private const string SummariesFolder = "Summaries";

    public DailySummaryViewModel(
        BottomBarViewModel shell,
        DailySummaryGenerator generator,
        ISettingsService settingsService,
        INotesService notes)
    {
        _shell = shell;
        _generator = generator;
        _settingsService = settingsService;
        _notes = notes;
    }

    /// <summary>Whether the sidebar shows the summary card instead of tiles + details.</summary>
    [ObservableProperty]
    private bool _isOpen;

    /// <summary>True while opencode writes the summary; drives the card's spinner.</summary>
    [ObservableProperty]
    private bool _isGenerating;

    /// <summary>The generated (or disk-loaded) markdown report; null when none yet.</summary>
    [ObservableProperty]
    private string? _summaryMarkdown;

    /// <summary>Header label — the day the report covers ("Monday, Sep 21, 2026").</summary>
    [ObservableProperty]
    private string? _summaryDateLabel;

    /// <summary>Nothing to report: no commits that day and a clean working tree.</summary>
    [ObservableProperty]
    private bool _hasNoActivity;

    public bool HasSummary => !string.IsNullOrWhiteSpace(SummaryMarkdown);

    public bool ShowEmptyState => !HasSummary && !IsGenerating;

    public string EmptyStateText => HasNoActivity
        ? "No commits or changes were recorded yesterday."
        : "No summary yet — press the wand to generate one.";

    /// <summary>The wand affordance: the OpenCode integration is on and no run is in flight.</summary>
    public bool CanGenerate => _shell.HasOpenCode && !IsGenerating;

    partial void OnIsGeneratingChanged(bool value)
    {
        OnPropertyChanged(nameof(CanGenerate));
        OnPropertyChanged(nameof(ShowEmptyState));
        GenerateCommand.NotifyCanExecuteChanged();
    }

    partial void OnHasNoActivityChanged(bool value) => OnPropertyChanged(nameof(EmptyStateText));

    /// <summary>
    /// Opens the card for the shell's selected repo: shows the day's summary file when
    /// one exists, generates it otherwise. The title-bar button's entry point.
    /// </summary>
    public async Task OpenAsync()
    {
        var repo = _shell.SelectedRepo;
        if (repo is null || IsGenerating) return;
        IsOpen = true;
        await LoadOrPrepareAsync(repo, Yesterday(), generateWhenMissing: true);
    }

    /// <summary>The shell's repo switched: drop any in-flight run and reload the open
    /// card for the new repo — its file shows when it exists, otherwise the card offers
    /// the wand (no auto-generation on a flip through the table).</summary>
    public async Task OnRepoSwitchedAsync()
    {
        _generator.Cancel();
        var repo = _shell.SelectedRepo;
        if (repo is null || !IsOpen) return;
        await LoadOrPrepareAsync(repo, Yesterday(), generateWhenMissing: false);
    }

    [RelayCommand(CanExecute = nameof(CanGenerate))]
    private async Task GenerateAsync()
    {
        var repo = _shell.SelectedRepo;
        if (repo is null || IsGenerating) return;
        IsOpen = true;
        await LoadOrPrepareAsync(repo, Yesterday(), generateWhenMissing: true);
    }

    [RelayCommand]
    private void Close() => IsOpen = false;

    /// <summary>Cancels any in-flight generation (repo switch, app shutdown).</summary>
    public void CancelGeneration() => _generator.Cancel();

    /// <summary>The shell's OpenCode availability flipped (settings save) — the wand re-queries.</summary>
    public void NotifyOpenCodeChanged()
    {
        OnPropertyChanged(nameof(CanGenerate));
        GenerateCommand.NotifyCanExecuteChanged();
    }

    /// <summary>Shows the day's file when it exists, then generates when allowed.</summary>
    private async Task LoadOrPrepareAsync(Repo repo, DateOnly day, bool generateWhenMissing)
    {
        HasNoActivity = false;
        SummaryDateLabel = DailySummaryGenerator.FormatDayLabel(day);

        var path = await ResolveSummaryPathAsync(repo, day);
        if (!ReferenceEquals(_shell.SelectedRepo, repo)) return;
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

        if (generateWhenMissing)
        {
            await GenerateCoreAsync(repo, day);
        }
        else
        {
            SetSummary(null);
        }
    }

    /// <summary>
    /// Gathers the repo's day activity, runs the prompt through opencode, shows the
    /// answer and persists it into the repo's notes store. A repo switch mid-run
    /// discards the result (the opencode child is already dead via kill-on-cancel).
    /// </summary>
    private async Task GenerateCoreAsync(Repo repo, DateOnly day)
    {
        if (!_shell.HasOpenCode)
        {
            _shell.Notifications.Show("Enable the OpenCode integration to generate summaries", NotificationKind.Warning);
            return;
        }

        var token = _generator.Begin();
        IsGenerating = true;
        try
        {
            var activity = await _shell.GitStatusService.GetDayActivityAsync(repo, day);
            if (!ReferenceEquals(_shell.SelectedRepo, repo)) return;
            if (activity.IsEmpty)
            {
                SetSummary(null);
                HasNoActivity = true;
                return;
            }

            var summary = await _generator.TryGenerateAsync(
                token, activity, repo.Name, day,
                _shell.ReposSettings.OpenCodeExecutable,
                _shell.ResolveWandModel());
            if (!ReferenceEquals(_shell.SelectedRepo, repo)) return;
            if (summary is null)
            {
                _shell.Notifications.Show("Could not generate yesterday's summary", NotificationKind.Error);
                return;
            }

            SetSummary(summary);
            await SaveAsync(repo, day, summary);
        }
        catch (OperationCanceledException)
        {
            // Repo switch or app shutdown: the opencode child is already dead, the
            // card stays as-is — drop the run without an error toast.
        }
        finally
        {
            _generator.End();
            IsGenerating = false;
        }
    }

    private async Task SaveAsync(Repo repo, DateOnly day, string summary)
    {
        try
        {
            var path = await ResolveSummaryPathAsync(repo, day);
            if (path is null) return;
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            await _notes.WriteNoteAsync(path, summary);
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Daily summary save failed for {Repo}", repo.Name);
        }
    }

    /// <summary>The day's file in the repo's notes store:
    /// <c>&lt;store&gt;/&lt;repo&gt;/Summaries/&lt;yyyy-MM-dd&gt;.md</c>.</summary>
    private async Task<string?> ResolveSummaryPathAsync(Repo repo, DateOnly day)
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var storeRoot = _notes.ResolveStoreRoot(settings.General?.NotesStorePath);
            return Path.Combine(
                _notes.GetRepoNotesRoot(storeRoot, repo.Name),
                SummariesFolder,
                $"{day:yyyy-MM-dd}.md");
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Daily summary path resolution failed for {Repo}", repo.Name);
            return null;
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
