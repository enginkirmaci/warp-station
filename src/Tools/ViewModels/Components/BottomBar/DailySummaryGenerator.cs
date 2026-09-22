using System.Globalization;
using System.Text;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// The title-bar sparkle's generator: shapes one repo's day activity (commits +
/// uncommitted changes) into the daily-summary prompt and runs it through opencode
/// via <see cref="OpenCodeWandGenerator"/>. Snapshot rule shared with the other
/// wands: everything the prompt needs is captured before the run — a repo switch
/// mid-run must not leak into the answer (the caller discards stale results too).
/// </summary>
public sealed class DailySummaryGenerator : OpenCodeWandGenerator
{
    private readonly IDailySummaryPromptService _promptService;

    /// <summary>The day's commits are inherently bounded; the caps keep a pathological
    /// history (or a huge working tree) from ballooning the prompt.</summary>
    private const int MaxCommitLines = 40;
    private const int MaxWorkingChangeLines = 200;
    private const int MaxWorkingChangeChars = 8000;

    /// <summary>Answer cap for generated markdown — long, but not a runaway.</summary>
    private const int MaxSummaryLength = 20000;

    public DailySummaryGenerator(
        IOpenCodeRunService openCodeRunService,
        IDailySummaryPromptService promptService)
        : base(openCodeRunService)
        => _promptService = promptService;

    /// <summary>
    /// Runs the generation for the snapshotted day activity; the cleaned markdown, or
    /// null when the CLI fails / nothing usable came back.
    /// </summary>
    public async Task<string?> TryGenerateAsync(
        CancellationToken cancellationToken,
        GitDayActivity activity,
        string repoName,
        DateOnly day,
        string executable,
        string? model)
    {
        var prompt = _promptService.BuildPrompt(
            repoName,
            string.IsNullOrWhiteSpace(activity.BranchName) ? "(unknown)" : activity.BranchName!,
            FormatDayLabel(day),
            FormatCommitLog(activity.Commits),
            FormatWorkingChanges(activity.UncommittedFiles));
        return await RunAsync(cancellationToken, executable, model, prompt, MaxSummaryLength);
    }

    /// <summary>The report's date label ("Monday, Sep 21, 2026").</summary>
    public static string FormatDayLabel(DateOnly day)
        => day.ToString("dddd, MMM d, yyyy", CultureInfo.InvariantCulture);

    private static string FormatCommitLog(IReadOnlyList<GitDayCommit> commits)
    {
        if (commits.Count == 0) return "(no commits that day)";

        var builder = new StringBuilder();
        foreach (var commit in commits.Take(MaxCommitLines))
        {
            builder.Append("- ").Append(commit.Hash, 0, 7)
                .Append(' ').Append(commit.When.LocalDateTime.ToString("HH:mm", CultureInfo.InvariantCulture))
                .Append(' ').Append(commit.Subject.ReplaceLineEndings(" "));
            if (commit.Files.Count > 0)
            {
                builder.Append(" — ").Append(DescribeCounts(commit.Files));
            }

            builder.Append('\n');
        }

        if (commits.Count > MaxCommitLines)
        {
            builder.Append("… and ").Append(commits.Count - MaxCommitLines).Append(" more commits\n");
        }

        return builder.ToString().TrimEnd();
    }

    private static string DescribeCounts(IReadOnlyList<GitChangedFile> files)
    {
        var counted = files.Where(f => f.HasCounts).ToList();
        var additions = counted.Sum(f => f.Additions ?? 0);
        var deletions = counted.Sum(f => f.Deletions ?? 0);
        return $"{files.Count} file(s), +{additions}/−{deletions}";
    }

    private static string FormatWorkingChanges(IReadOnlyList<GitChangedFile> files)
    {
        if (files.Count == 0) return "(working tree clean — nothing uncommitted)";

        var builder = new StringBuilder();
        foreach (var file in files.Take(MaxWorkingChangeLines))
        {
            builder.Append("- ").Append(file.DisplayStatusCode).Append("  ").Append(file.Path);
            if (file.HasCounts)
            {
                builder.Append(" (").Append(file.AddedText).Append('/').Append(file.RemovedText).Append(')');
            }

            builder.Append('\n');
        }

        if (files.Count > MaxWorkingChangeLines)
        {
            builder.Append("… and ").Append(files.Count - MaxWorkingChangeLines).Append(" more files\n");
        }

        var text = builder.ToString().TrimEnd();
        if (text.Length > MaxWorkingChangeChars)
        {
            text = text[..MaxWorkingChangeChars] + "\n… (list truncated)";
        }

        return text;
    }
}
