namespace Tools.Library.Entities;

/// <summary>
/// One working-tree change of a repo, parsed from
/// <c>git status --porcelain=v2 --untracked-files=all</c> for the bottom bar's Changes
/// tab. <see cref="StatusCode"/> is the porcelain status ("M", "A", "?", …); for
/// staged+unstaged combinations the two XY characters are kept verbatim.
/// <see cref="Additions"/>/<see cref="Deletions"/> come from the staged+unstaged
/// numstat diff and are null for untracked (and binary) files, which have no counts.
/// </summary>
public sealed record GitChangedFile(
    string Path,
    string StatusCode,
    int? Additions = null,
    int? Deletions = null)
{
    // The delta strings below are rendered on every repo row, so they are cached:
    // the counts are positional (get-only, fixed at construction), a lazily built
    // string never goes stale and needs no invalidation.
    private string? _addedText;
    private string? _removedText;

    /// <summary>Whether the file carries numstat line counts (untracked/binary do not).</summary>
    public bool HasCounts => Additions is not null || Deletions is not null;

    /// <summary>
    /// Which side of the index the row currently renders in — the merged Changes
    /// list branches its +/− action button on it (unstaged rows stage, staged rows
    /// unstage). Set by the bar when it builds the section rows; records are
    /// recreated on every status load, so nothing to invalidate.
    /// </summary>
    public bool IsStaged { get; set; }

    /// <summary>
    /// The status letter as the UI shows it — "U" for untracked (VS Code's letter), the
    /// porcelain code otherwise ("?" is git's raw untracked marker, never displayed).
    /// </summary>
    public string DisplayStatusCode => StatusCode == "?" ? "U" : StatusCode;

    /// <summary>The "+N" additions half of the delta; null without counts. Cached.</summary>
    public string? AddedText => _addedText ??= Additions is null ? null : $"+{Additions}";

    /// <summary>The "−N" deletions half of the delta; null without counts. Cached.</summary>
    public string? RemovedText => _removedText ??= Deletions is null ? null : $"−{Deletions}";
}
