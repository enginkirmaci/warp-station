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
    /// <summary>Whether the file carries numstat line counts (untracked/binary do not).</summary>
    public bool HasCounts => Additions is not null || Deletions is not null;

    /// <summary>
    /// The status letter as the UI shows it — "U" for untracked (VS Code's letter), the
    /// porcelain code otherwise ("?" is git's raw untracked marker, never displayed).
    /// </summary>
    public string DisplayStatusCode => StatusCode == "?" ? "U" : StatusCode;

    /// <summary>The "+N" additions half of the delta; null without counts.</summary>
    public string? AddedText => Additions is null ? null : $"+{Additions}";

    /// <summary>The "−N" deletions half of the delta; null without counts.</summary>
    public string? RemovedText => Deletions is null ? null : $"−{Deletions}";
}
