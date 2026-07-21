using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;

namespace Tools.Library.Entities;

/// <summary>
/// Represents a single repository (a discovered .git folder's parent) in the
/// application. The solution file, if present, becomes a property of the repo;
/// otherwise the Visual Studio action is disabled for it.
/// </summary>
public partial class Repo : ObservableObject
{
    /// <summary>
    /// Gets or sets the display name (the repo folder name).
    /// </summary>
    public string? Name { get; set; }

    /// <summary>
    /// Gets or sets the absolute path of the repo folder (the .git folder's parent).
    /// Used as the stable identity when merging user-defined tags across rescans.
    /// </summary>
    public string? FolderPath { get; set; }

    /// <summary>
    /// Gets or sets the absolute path of the solution file, if one was found.
    /// When <c>null</c> the repo has no Visual Studio action.
    /// </summary>
    public string? SolutionPath { get; set; }

    /// <summary>
    /// Gets the tags assigned to this repo. Each entry is a <see cref="RepoTag"/> that
    /// back-references this repo so chip commands can act on a single parameter.
    /// Auto-tags (e.g. <c>platform</c>) are recomputed on each scan; user tags are
    /// persisted in the cache and merged back after a rescan by matching
    /// <see cref="FolderPath"/>.
    /// </summary>
    public ObservableCollection<RepoTag> Tags { get; set; } = new();

    /// <summary>
    /// Convenience: adds a tag by name, ignoring duplicates (case-insensitive).
    /// </summary>
    public void AddTag(string name)
    {
        var trimmed = name?.Trim();
        if (string.IsNullOrEmpty(trimmed)) return;
        if (Tags.Any(t => string.Equals(t.Name, trimmed, StringComparison.OrdinalIgnoreCase))) return;
        Tags.Add(new RepoTag(this, trimmed));
    }

    /// <summary>
    /// Convenience: removes a tag by name (case-insensitive). Returns true if removed.
    /// </summary>
    public bool RemoveTag(string name)
    {
        var toRemove = Tags
            .Where(t => string.Equals(t.Name, name, StringComparison.OrdinalIgnoreCase))
            .ToList();
        foreach (var tag in toRemove)
            Tags.Remove(tag);
        return toRemove.Count > 0;
    }

    /// <summary>
    /// The reserved tag toggled by the star affordance. Defined here (rather than in
    /// <c>RepoService</c>) so <see cref="IsFavorite"/> can reference it without the
    /// entities layer depending on the services layer.
    /// </summary>
    public const string FavoritesTag = "favorites";

    /// <summary>
    /// The auto-tag applied by the scanner to repos whose folder path matches the
    /// configured platform folder name. Defined here so both the scanner (services
    /// layer) and UI consumers can reference it without coupling.
    /// </summary>
    public const string PlatformTag = "platform";

    /// <summary>
    /// True when the reserved <c>favorites</c> tag is present.
    /// </summary>
    public bool IsFavorite
        => Tags.Any(t => string.Equals(t.Name, FavoritesTag, StringComparison.OrdinalIgnoreCase));

    /// <summary>
    /// The current branch name reported by the local git status check, pushed by
    /// <c>IGitStatusService</c>. Shown next to the Git section header on the repo card.
    /// <c>null</c> until the first check completes (or when it fails). Runtime-only;
    /// not persisted.
    /// </summary>
    [ObservableProperty]
    private string? _gitBranchName;

    /// <summary>
    /// Number of working-tree changes (modified, renamed, unmerged and untracked files)
    /// reported by <c>git status --porcelain=v2 --untracked-files=all</c>. Every
    /// untracked file counts individually (git's default directory collapsing is off).
    /// Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitModifiedCount;

    /// <summary>
    /// Number of local commits ahead of the upstream branch (to push). Zero when the
    /// branch has no upstream. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitToPushCount;

    /// <summary>
    /// Number of upstream commits the local branch is behind (to pull). Zero when the
    /// branch has no upstream. Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private int _gitToPullCount;

    /// <summary>
    /// Whether the first git status check has completed for this repo. The card shows a
    /// "checking…" placeholder until this flips to <see langword="true"/>, so the UI
    /// renders instantly and the counts fill in as the background checks finish.
    /// Runtime-only; not persisted.
    /// </summary>
    [ObservableProperty]
    private bool _gitStatusLoaded;
}
