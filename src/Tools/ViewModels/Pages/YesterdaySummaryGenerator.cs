using System.Globalization;
using System.Text;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.ViewModels.Pages;

/// <summary>One repository's shaped day activity for the all-repos summary prompt.</summary>
public sealed record RepoDayActivity(string RepoName, GitDayActivity Activity);

/// <summary>
/// The Yesterday's Summary tool's generator: shapes every active repository's day
/// activity (commits + uncommitted changes) into one daily-summary prompt and runs it
/// through opencode via <see cref="OpenCodeWandGenerator"/>. Everything the prompt
/// needs is snapshotted before the run — live repo state cannot leak into the answer.
/// </summary>
public sealed class YesterdaySummaryGenerator : OpenCodeWandGenerator
{
    private readonly IDailySummaryPromptService _promptService;

    /// <summary>Per-repo caps keep one noisy repository from drowning the rest; the
    /// repo cap keeps a large tracked list from ballooning the prompt.</summary>
    private const int MaxRepos = 12;
    private const int MaxCommitLinesPerRepo = 20;
    private const int MaxWorkingChangeLinesPerRepo = 80;
    private const int MaxWorkingChangeCharsPerRepo = 4000;

    /// <summary>Answer cap for generated markdown — long, but not a runaway.</summary>
    private const int MaxSummaryLength = 30000;

    public YesterdaySummaryGenerator(
        IOpenCodeRunService openCodeRunService,
        IDailySummaryPromptService promptService)
        : base(openCodeRunService)
        => _promptService = promptService;

    /// <summary>
    /// Runs the generation for the snapshotted per-repo activities; the cleaned
    /// markdown, or null when the CLI fails / nothing usable came back.
    /// </summary>
    public async Task<string?> TryGenerateAsync(
        CancellationToken cancellationToken,
        IReadOnlyList<RepoDayActivity> activities,
        string dateLabel,
        string executable,
        string? model)
    {
        var prompt = _promptService.BuildPrompt(dateLabel, FormatRepositoriesBlock(activities));
        return await RunAsync(cancellationToken, executable, model, prompt, MaxSummaryLength);
    }

    /// <summary>The report's date label ("Monday, Sep 21, 2026").</summary>
    public static string FormatDayLabel(DateOnly day)
        => day.ToString("dddd, MMM d, yyyy", CultureInfo.InvariantCulture);

    private static string FormatRepositoriesBlock(IReadOnlyList<RepoDayActivity> activities)
    {
        var builder = new StringBuilder();
        foreach (var entry in activities.Take(MaxRepos))
        {
            builder.Append("## ").Append(entry.RepoName);
            if (!string.IsNullOrWhiteSpace(entry.Activity.BranchName))
            {
                builder.Append(" (branch ").Append(entry.Activity.BranchName).Append(')');
            }

            builder.Append('\n')
                .Append("Commits:\n").Append(FormatCommitLog(entry.Activity.Commits)).Append('\n')
                .Append("Uncommitted changes:\n").Append(FormatWorkingChanges(entry.Activity.UncommittedFiles))
                .Append('\n').Append('\n');
        }

        if (activities.Count > MaxRepos)
        {
            builder.Append("(… and ").Append(activities.Count - MaxRepos).Append(" more active repositories omitted)\n");
        }

        return builder.ToString().TrimEnd();
    }

    private static string FormatCommitLog(IReadOnlyList<GitDayCommit> commits)
    {
        if (commits.Count == 0) return "(no commits that day)";

        var builder = new StringBuilder();
        foreach (var commit in commits.Take(MaxCommitLinesPerRepo))
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

        if (commits.Count > MaxCommitLinesPerRepo)
        {
            builder.Append("… and ").Append(commits.Count - MaxCommitLinesPerRepo).Append(" more commits\n");
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
        foreach (var file in files.Take(MaxWorkingChangeLinesPerRepo))
        {
            builder.Append("- ").Append(file.DisplayStatusCode).Append("  ").Append(file.Path);
            if (file.HasCounts)
            {
                builder.Append(" (").Append(file.AddedText).Append('/').Append(file.RemovedText).Append(')');
            }

            builder.Append('\n');
        }

        if (files.Count > MaxWorkingChangeLinesPerRepo)
        {
            builder.Append("… and ").Append(files.Count - MaxWorkingChangeLinesPerRepo).Append(" more files\n");
        }

        var text = builder.ToString().TrimEnd();
        if (text.Length > MaxWorkingChangeCharsPerRepo)
        {
            text = text[..MaxWorkingChangeCharsPerRepo] + "\n… (list truncated)";
        }

        return text;
    }
}
