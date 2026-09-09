using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Helpers;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// One entry of the Repos page sort selector: the sort mode plus its display label.
/// A labeled wrapper (rather than binding the raw enum) keeps the dropdown text in
/// one place and works with compiled bindings without a value converter.
/// </summary>
public sealed record RepoSortOption(RepoSortMode Mode, string Label);

/// <summary>
/// The Repos page's list projection: the search + tag filter state, the sort
/// selection, the filtered/sorted <see cref="FilteredRepos"/> and the debounces that
/// coalesce bursts of keystrokes and scan events into one in-place list sync. The
/// page view model owns the lifecycle (page load, refresh, navigate-from) and calls
/// <see cref="Rebuild"/>/<see cref="Detach"/>; the projection owns everything between.
/// </summary>
public partial class RepoListProjection : ObservableObject
{
    private readonly IRepoService _repoService;

    /// <summary>
    /// Debounce timers for the filter and the service-changed handler. A burst of typing or
    /// the several <c>Changed</c> raises a single scan produces each cancel the pending
    /// callback and restart the window, so only one in-place <see cref="ApplyFilter"/> runs
    /// per burst instead of tearing down the list per keystroke / per event.
    /// </summary>
    private readonly UiDebounce _filterDebounce = new(FilterDebounceMs);
    private readonly UiDebounce _changedDebounce = new(ChangedDebounceMs);

    /// <summary>Idle window for the search-box filter before the list is re-synced.</summary>
    private const int FilterDebounceMs = 150;

    /// <summary>
    /// Idle window for coalescing the multiple <c>Changed</c> raises a single scan emits
    /// (start, data-ready, finally) into one rebuild.
    /// </summary>
    private const int ChangedDebounceMs = 100;

    /// <summary>
    /// The repos currently wired to <see cref="OnRepoPropertyChanged"/> for live
    /// re-sorting. The service raises <c>Changed</c> only around scans, but the
    /// background git status / GitHub passes push their results straight onto the
    /// entities afterwards — without listening to the entities, a Last-activity/Changes
    /// sort would keep its pre-probe order. Rebuilt after every scan because a rescan
    /// can replace the repo instances.
    /// </summary>
    private readonly HashSet<Repo> _sortObservedRepos = new();

    public RepoListProjection(IRepoService repoService)
    {
        _repoService = repoService;
    }

    /// <summary>Observed repo property changes, forwarded after the projection's own
    /// sort handling — the page's header totals fold the count kinds they care about.</summary>
    public event Action<Repo, PropertyChangedEventArgs>? RepoPropertyObserved;

    /// <summary>Raised at the end of every <see cref="Rebuild"/> — the page re-seeds
    /// its header totals there (a rescan can replace the repo instances).</summary>
    public event Action? Rebuilt;

    /// <summary>The sort orders offered in the toolbar selector, in dropdown order.</summary>
    public static IReadOnlyList<RepoSortOption> SortOptions { get; } = new[]
    {
        new RepoSortOption(RepoSortMode.Name, "Name"),
        new RepoSortOption(RepoSortMode.LastActivity, "Last activity"),
        new RepoSortOption(RepoSortMode.Changes, "Changes"),
        new RepoSortOption(RepoSortMode.PullRequests, "Pull requests"),
        new RepoSortOption(RepoSortMode.Issues, "Issues"),
    };

    [ObservableProperty]
    private string _filterText = string.Empty;

    /// <summary>
    /// The currently selected entry of the toolbar sort selector. Seeded from the
    /// persisted <see cref="ReposSettings.SortMode"/> on page load; a user pick
    /// re-orders the list immediately and reports <see cref="SortModePicked"/>.
    /// </summary>
    [ObservableProperty]
    private RepoSortOption _selectedSortOption = SortOptions[0];

    /// <summary>Raised when the user picks a sort option — the page persists the mode.</summary>
    public event Action<RepoSortMode>? SortModePicked;

    [ObservableProperty]
    private ObservableCollection<Repo> _filteredRepos = new();

    /// <summary>
    /// The checkable tag list shown in the left filter panel. Rebuilt from
    /// <see cref="IRepoService.AllTags"/> whenever the service changes, preserving
    /// existing check states by tag name so checking a tag survives a rescan.
    /// </summary>
    [ObservableProperty]
    private ObservableCollection<TagFilter> _tagFilters = new();

    /// <summary>Whether the tracked-repo list is empty altogether — the empty-state
    /// overlay's "no repositories yet" variant (vs "the filters hid everything").</summary>
    [ObservableProperty]
    private bool _hasNoRepos;

    /// <summary>Whether a search term or checked tag filter is currently applied —
    /// drives the empty-state overlay's "Clear filters" action.</summary>
    [ObservableProperty]
    private bool _isFilterActive;

    /// <summary>Whether the table has no rows at all (overlay visibility); raised by
    /// <see cref="ApplyFilter"/> after the projection sync.</summary>
    public bool ShowReposEmptyNote => FilteredRepos.Count == 0;

    /// <summary>
    /// The one rebuild pass a scan burst collapses into: re-sync the tag list,
    /// re-wire the live-re-sort listeners to the (possibly replaced) repo instances
    /// and re-sync the list.
    /// </summary>
    public void Rebuild()
    {
        RebuildTagFilters();
        RefreshSortListeners();
        ApplyFilter();
        Rebuilt?.Invoke();
    }

    /// <summary>Coalesces a burst of <see cref="IRepoService.Changed"/> raises into a
    /// single <see cref="Rebuild"/> on the UI thread.</summary>
    public void ScheduleChangedRebuild() => _changedDebounce.Debounce(Rebuild);

    /// <summary>Drops a pending changed-rebuild — the caller settles the projection
    /// itself when it wants the list final before its busy state clears.</summary>
    public void CancelPendingRebuild() => _changedDebounce.Cancel();

    /// <summary>
    /// Detaches from the singleton-cached repos and cancels the debounces: the Transient
    /// page VM must not be kept alive by them nor receive further state changes.
    /// </summary>
    public void Detach()
    {
        foreach (var repo in _sortObservedRepos)
            repo.PropertyChanged -= OnRepoPropertyChanged;
        _sortObservedRepos.Clear();

        _filterDebounce.Dispose();
        _changedDebounce.Dispose();
    }

    partial void OnFilterTextChanged(string value) => _filterDebounce.Debounce(ApplyFilter);

    partial void OnSelectedSortOptionChanged(RepoSortOption? value)
    {
        if (value is null) return;
        ApplyFilter();
        SortModePicked?.Invoke(value.Mode);
    }

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

    /// <summary>
    /// Reconciles the checkable tag list with <see cref="IRepoService.AllTags"/>: reuses
    /// the existing <see cref="TagFilter"/> instance per tag name (its check state
    /// survives untouched), removes only vanished tags and adds only new ones. Most
    /// debounced scan raises find an unchanged tag set, where the merge is a no-op — the
    /// tags ItemsControl then rebuilds nothing, where the previous wholesale rebuild
    /// churned every checkbox per raise.
    /// </summary>
    private void RebuildTagFilters()
    {
        var tags = _repoService.AllTags
            .OrderBy(t => t, StringComparer.OrdinalIgnoreCase)
            .ToList();

        // Common case: same names in the same (sorted) order — keep the existing
        // instances and collection untouched.
        if (TagFilters.Count == tags.Count)
        {
            var identical = true;
            for (var i = 0; i < tags.Count; i++)
            {
                if (string.Equals(TagFilters[i].Name, tags[i], StringComparison.OrdinalIgnoreCase)) continue;
                identical = false;
                break;
            }

            if (identical) return;
        }

        var reusable = new Dictionary<string, TagFilter>(StringComparer.OrdinalIgnoreCase);
        foreach (var filter in TagFilters)
            reusable[filter.Name] = filter;

        var membershipChanged = false;
        var merged = new List<TagFilter>(tags.Count);
        foreach (var name in tags)
        {
            if (reusable.Remove(name, out var filter))
            {
                merged.Add(filter);
            }
            else
            {
                merged.Add(new TagFilter(name));
                membershipChanged = true; // a tag appeared
            }
        }

        // Whatever was not merged back belongs to a tag that vanished.
        membershipChanged |= reusable.Count > 0;

        if (membershipChanged)
        {
            // The rare case (a rescan added/removed tags): replace the collection, with
            // the surviving tags keeping their instances — and thus their check states —
            // without re-setting them.
            TagFilters = new ObservableCollection<TagFilter>(merged);
            return;
        }

        // Same tags, different order (virtually never — both sides sort the same way):
        // reorder the existing collection with Move notifications instead of replacing
        // it, so the checkboxes keep their containers.
        for (var i = 0; i < merged.Count; i++)
        {
            var target = merged[i];
            if (ReferenceEquals(TagFilters[i], target)) continue;

            for (var j = i; j < TagFilters.Count; j++)
            {
                if (!ReferenceEquals(TagFilters[j], target)) continue;
                TagFilters.Move(j, i);
                break;
            }
        }
    }

    /// <summary>
    /// Re-wires the live-re-sort listeners to the repos the service currently knows.
    /// Idempotent per pass; called on page load and after every scan so replaced repo
    /// instances don't leave the set holding (and keeping alive) stale ones.
    /// </summary>
    private void RefreshSortListeners()
    {
        foreach (var repo in _sortObservedRepos)
            repo.PropertyChanged -= OnRepoPropertyChanged;
        _sortObservedRepos.Clear();

        foreach (var repo in _repoService.Repos)
            _sortObservedRepos.Add(repo);

        foreach (var repo in _sortObservedRepos)
            repo.PropertyChanged += OnRepoPropertyChanged;
    }

    private void OnRepoPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        // Only the properties the sort keys read can change the ordering; ignoring the
        // rest (branch name, Azure counts, …) keeps a full probe pass from re-sorting for
        // nothing. Reuses the filter debounce so a burst of probe completions collapses
        // into one re-order.
        if (e.PropertyName is nameof(Repo.GitLastCommitAt)
            or nameof(Repo.GitModifiedCount)
            or nameof(Repo.GitToPushCount)
            or nameof(Repo.GitToPullCount))
        {
            _filterDebounce.Debounce(ApplyFilter);
        }

        if (sender is Repo repo)
        {
            RepoPropertyObserved?.Invoke(repo, e);
        }
    }

    /// <summary>
    /// Orders the filtered repos: favorites always float to the top (the star is a pin,
    /// in every mode), then the selected sort mode orders the rest, with the name as the
    /// stable tiebreaker. <see cref="Repo.GitLastCommitAt"/> nulls (not yet probed or no
    /// commits) sort last because DateTimeOffset? ascending puts null smallest and the
    /// ordering is descending.
    /// </summary>
    private IOrderedEnumerable<Repo> SortRepos(IEnumerable<Repo> repos)
    {
        // Repo.IsFavorite walks the Tags collection with .Any per read, and a comparison
        // sort evaluates its primary key O(n log n) times — snapshot the flag once per
        // repo before sorting and capture the snapshot in the comparator. One tag walk
        // per repo, identical ordering (bool descending, stable, as before).
        var snapshot = repos.ToList();
        var favoriteFlags = new Dictionary<Repo, bool>(snapshot.Count);
        foreach (var repo in snapshot)
            favoriteFlags[repo] = repo.IsFavorite;

        var favoritesFirst = snapshot.OrderByDescending(r => favoriteFlags[r]);
        return SelectedSortOption.Mode switch
        {
            RepoSortMode.LastActivity => favoritesFirst
                .ThenByDescending(r => r.GitLastCommitAt)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            RepoSortMode.Changes => favoritesFirst
                .ThenByDescending(r => r.GitModifiedCount + r.GitToPushCount + r.GitToPullCount)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            RepoSortMode.PullRequests => favoritesFirst
                .ThenByDescending(r => r.GitHubPrCount)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            RepoSortMode.Issues => favoritesFirst
                .ThenByDescending(r => r.GitHubIssueCount)
                .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
            _ => favoritesFirst.ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase),
        };
    }

    private void ApplyFilter()
    {
        var filter = FilterText?.Trim();
        var repos = _repoService.Repos;
        var checkedTags = TagFilters
            .Where(t => t.IsChecked)
            .Select(t => t.Name)
            .ToHashSet(StringComparer.OrdinalIgnoreCase);

        // Empty-state overlays: "no repositories yet" vs "no matches" need to know
        // whether the tracked set itself is empty and whether a filter is applied.
        var noRepos = repos.Count == 0;
        if (HasNoRepos != noRepos)
        {
            HasNoRepos = noRepos;
        }

        var filterActive = checkedTags.Count > 0 || !string.IsNullOrWhiteSpace(filter);
        if (IsFilterActive != filterActive)
        {
            IsFilterActive = filterActive;
        }

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

        // Favorites always float to the top, then the selected sort mode (see SortRepos).
        var ordered = SortRepos(result).ToList();

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

        OnPropertyChanged(nameof(ShowReposEmptyNote));
    }
}
