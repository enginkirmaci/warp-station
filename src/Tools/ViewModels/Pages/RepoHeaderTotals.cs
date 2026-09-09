using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Tools.Library.Entities;

namespace Tools.ViewModels.Pages;

/// <summary>
/// The Repos page's running header totals (open pull requests / issues / uncommitted
/// changes across all known repos). A repo count change folds its delta in (see
/// <see cref="HandleRepoProperty"/>) instead of re-summing every repo per event — a
/// full probe pass used to cost O(N²) enumerations of the repo set. Recomputed from
/// scratch and re-seeded whenever the repo set may have been replaced (see
/// <see cref="Recalculate"/>), so list membership changes cannot drift them.
/// </summary>
public sealed class RepoHeaderTotals : ObservableObject
{
    private int _totalPrCount;
    private int _totalIssueCount;
    private int _totalModifiedCount;

    /// <summary>
    /// Per-repo last-known contributions to the totals above: the incremental fold needs
    /// the previous value to subtract and <see cref="PropertyChangedEventArgs"/> carries
    /// none. Keyed by instance (repos are shared references, no value equality); cleared
    /// and re-seeded together with the totals.
    /// </summary>
    private readonly Dictionary<Repo, int> _prTotalContributions = new();
    private readonly Dictionary<Repo, int> _issueTotalContributions = new();
    private readonly Dictionary<Repo, int> _modifiedTotalContributions = new();

    /// <summary>
    /// Guards the totals and their contribution dictionaries: the git/GitHub probes push
    /// counts from background continuations, and a read-modify-write fold must not
    /// interleave. Getters read plain ints (atomic), so the worst case under a concurrent
    /// fold is a one-frame-stale total — which the previous per-event re-sum could show
    /// just the same.
    /// </summary>
    private readonly object _totalsLock = new();

    /// <summary>
    /// Total open pull requests across all known repos — the at-a-glance summary beside
    /// the page title. Unloaded and non-GitHub repos contribute zero.
    /// </summary>
    public int GitHubTotalPrCount => _totalPrCount;

    /// <summary>Total open issues across all known repos. See <see cref="GitHubTotalPrCount"/>.</summary>
    public int GitHubTotalIssueCount => _totalIssueCount;

    /// <summary>
    /// Total uncommitted file changes across all known repos — the red third of the
    /// header stat row. Starts at zero and fills in as the background git status probes
    /// push their counts onto the entities.
    /// </summary>
    public int GitTotalModifiedCount => _totalModifiedCount;

    /// <summary>
    /// Recomputes the running header totals from the current repo set in one pass and
    /// re-seeds the per-repo contributions to match, then re-raises every total. Called
    /// after the repo set may have been replaced wholesale (initial load, rescan): the
    /// fresh entities start at zero, so previously non-zero stats must drop without any
    /// single entity carrying a change notification — and only a full recompute here
    /// keeps the incremental folds correct across list membership changes.
    /// </summary>
    public void Recalculate(IReadOnlyList<Repo> repos)
    {
        lock (_totalsLock)
        {
            _prTotalContributions.Clear();
            _issueTotalContributions.Clear();
            _modifiedTotalContributions.Clear();

            var pr = 0;
            var issues = 0;
            var modified = 0;
            foreach (var repo in repos)
            {
                pr += repo.GitHubPrCount;
                issues += repo.GitHubIssueCount;
                modified += repo.GitModifiedCount;
                _prTotalContributions[repo] = repo.GitHubPrCount;
                _issueTotalContributions[repo] = repo.GitHubIssueCount;
                _modifiedTotalContributions[repo] = repo.GitModifiedCount;
            }

            _totalPrCount = pr;
            _totalIssueCount = issues;
            _totalModifiedCount = modified;
        }

        OnPropertyChanged(nameof(GitHubTotalPrCount));
        OnPropertyChanged(nameof(GitHubTotalIssueCount));
        OnPropertyChanged(nameof(GitTotalModifiedCount));
    }

    /// <summary>Drops the per-repo contributions so detached (navigate-from) repos are
    /// not referenced from here anymore. The totals keep their last values until the
    /// next <see cref="Recalculate"/> re-seeds them.</summary>
    public void ClearContributions()
    {
        lock (_totalsLock)
        {
            _prTotalContributions.Clear();
            _issueTotalContributions.Clear();
            _modifiedTotalContributions.Clear();
        }
    }

    /// <summary>
    /// Folds one repo's count change into its running total: subtracts the repo's
    /// previous contribution (zero when never seen — a change arriving before the totals
    /// were seeded contributes its current value only) and adds the new value, then
    /// raises the changed total. The next <see cref="Recalculate"/> recomputes everything
    /// from the repo set, so no drift survives a rebuild. Returns whether the property
    /// was one of the totals' inputs.
    /// </summary>
    public bool HandleRepoProperty(Repo repo, string? propertyName)
    {
        if (propertyName is nameof(Repo.GitHubPrCount))
        {
            AdjustTotal(_prTotalContributions, repo, repo.GitHubPrCount, ref _totalPrCount);
            OnPropertyChanged(nameof(GitHubTotalPrCount));
        }
        else if (propertyName is nameof(Repo.GitHubIssueCount))
        {
            AdjustTotal(_issueTotalContributions, repo, repo.GitHubIssueCount, ref _totalIssueCount);
            OnPropertyChanged(nameof(GitHubTotalIssueCount));
        }
        else if (propertyName is nameof(Repo.GitModifiedCount))
        {
            AdjustTotal(_modifiedTotalContributions, repo, repo.GitModifiedCount, ref _totalModifiedCount);
            OnPropertyChanged(nameof(GitTotalModifiedCount));
        }
        else
        {
            return false;
        }

        return true;
    }

    private void AdjustTotal(Dictionary<Repo, int> contributions, Repo repo, int value, ref int total)
    {
        lock (_totalsLock)
        {
            contributions.TryGetValue(repo, out var previous);
            total += value - previous;
            contributions[repo] = value;
        }
    }
}
