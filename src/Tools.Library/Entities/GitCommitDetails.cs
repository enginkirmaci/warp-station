namespace Tools.Library.Entities;

/// <summary>
/// The full detail of one commit, parsed from <c>git show --numstat</c> for the
/// History drawer: every changed file with its added/deleted line counts (nulls for
/// binary files).
/// </summary>
public sealed record GitCommitDetails(
    string Hash,
    IReadOnlyList<GitChangedFile> Files)
{
    /// <summary>The "N files changed" count.</summary>
    public int FileCount => Files.Count;

    /// <summary>Total added lines over the counted files; null when none report counts.</summary>
    public int? Additions
    {
        get
        {
            var total = 0;
            foreach (var file in Files)
            {
                if (file.Additions is not { } additions) continue;
                total += additions;
            }
            return total;
        }
    }

    /// <summary>Total deleted lines over the counted files; null when none report counts.</summary>
    public int? Deletions
    {
        get
        {
            var total = 0;
            foreach (var file in Files)
            {
                if (file.Deletions is not { } deletions) continue;
                total += deletions;
            }
            return total;
        }
    }
}
