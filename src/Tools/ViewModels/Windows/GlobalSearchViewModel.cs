using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.ViewModels.Windows;

public enum GlobalSearchResultKind
{
    Repo,
    Note,
}

/// <summary>One row of the title-bar global search dropdown: a repository or a note
/// hit. Rows are rebuilt per search, so plain identity properties suffice.</summary>
public sealed class GlobalSearchResultViewModel
{
    public GlobalSearchResultViewModel(Repo repo)
    {
        Kind = GlobalSearchResultKind.Repo;
        Repo = repo;
    }

    public GlobalSearchResultViewModel(NotesSearchHit hit)
    {
        Kind = GlobalSearchResultKind.Note;
        Hit = hit;
    }

    public GlobalSearchResultKind Kind { get; }

    public bool IsRepo => Kind == GlobalSearchResultKind.Repo;

    public bool IsNote => Kind == GlobalSearchResultKind.Note;

    public string Title => Repo?.Name ?? Hit?.Name ?? string.Empty;

    /// <summary>The repo folder for repos, the note's store-relative path for notes.</summary>
    public string Subtitle => Repo?.FolderPath ?? Hit?.RelativePath ?? string.Empty;

    /// <summary>The matched content line (notes with a content hit only, else empty).</summary>
    public string Excerpt => Hit?.Excerpt ?? string.Empty;

    public bool HasExcerpt => Excerpt.Length > 0;

    public Repo? Repo { get; }

    public NotesSearchHit? Hit { get; }
}

/// <summary>
/// Drives the title bar's global search dropdown: repositories matched by name/paths
/// plus, from the whole notes store, notes matched by filename or content. Repos are
/// in-memory and land in the dropdown immediately; the note scan is off-thread and
/// appended when it completes. A generation counter discards results of searches a
/// newer keystroke superseded. Activation actions: a repo is selected in the bottom
/// bar (the row press behavior), a note opens in the Notes page via its pending-open
/// path.
/// </summary>
public partial class GlobalSearchViewModel : ObservableObject
{
    private readonly IRepoService _repos;
    private readonly INotesService _notes;
    private readonly ISettingsService _settings;
    private readonly BottomBarViewModel _bar;

    private const int RepoCap = 6;
    private const int NoteCap = 12;

    /// <summary>Files scanned per note search: ranking (filename hits first) wants a few
    /// more hits than the dropdown shows, but the scan must stay well under the store's
    /// full 2000-file cap to keep the dropdown snappy.</summary>
    private const int NoteScanCap = 40;

    private int _searchGeneration;

    /// <summary>The store root the latest search ran against (also the hit→repo
    /// resolution scope); empty until the first search with a non-empty term.</summary>
    private string _storeRoot = string.Empty;

    public GlobalSearchViewModel(
        IRepoService repos,
        INotesService notes,
        ISettingsService settings,
        BottomBarViewModel bar)
    {
        _repos = repos;
        _notes = notes;
        _settings = settings;
        _bar = bar;
    }

    [ObservableProperty]
    private ObservableCollection<GlobalSearchResultViewModel> _results = new();

    /// <summary>The last search completed with no match at all (the dropdown's
    /// "no matches" footer); false while a search is in flight.</summary>
    [ObservableProperty]
    private bool _hasNoResults;

    public async Task SearchAsync(string term)
    {
        var generation = ++_searchGeneration;
        var query = term.Trim();

        if (query.Length == 0)
        {
            Reset();
            return;
        }

        // Repos are in-memory: replace the rows right away so the dropdown reacts on
        // the first keystroke, then let the note scan append its matches.
        var repoRows = _repos.Repos
            .Where(r =>
                r.Name?.Contains(query, StringComparison.OrdinalIgnoreCase) == true
                || r.FolderPath?.Contains(query, StringComparison.OrdinalIgnoreCase) == true
                || r.SolutionPath?.Contains(query, StringComparison.OrdinalIgnoreCase) == true)
            .OrderByDescending(r => r.IsFavorite)
            .ThenBy(r => r.Name, StringComparer.OrdinalIgnoreCase)
            .Take(RepoCap)
            .Select(r => new GlobalSearchResultViewModel(r))
            .ToList();

        if (generation != _searchGeneration)
        {
            return;
        }

        HasNoResults = false;
        Results = new ObservableCollection<GlobalSearchResultViewModel>(repoRows);

        try
        {
            var settings = await _settings.GetSettingsAsync();
            if (generation != _searchGeneration)
            {
                return;
            }

            _storeRoot = _notes.ResolveStoreRoot(settings.General?.NotesStorePath);
            var hits = await _notes.SearchAsync(_storeRoot, query, maxHits: NoteScanCap);
            if (generation != _searchGeneration)
            {
                return;
            }

            // Filename matches first, then content matches by store-relative path.
            var noteRows = hits
                .OrderByDescending(h => h.Name.Contains(query, StringComparison.OrdinalIgnoreCase))
                .ThenBy(h => h.RelativePath, StringComparer.OrdinalIgnoreCase)
                .Take(NoteCap)
                .Select(h => new GlobalSearchResultViewModel(h));

            Results = new ObservableCollection<GlobalSearchResultViewModel>(repoRows.Concat(noteRows));
            HasNoResults = Results.Count == 0;
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Global search note scan failed for '{Term}'", query);
        }
    }

    /// <summary>Drops the dropdown content (empty term, popup dismissal, activation).</summary>
    public void Reset()
    {
        _searchGeneration++;
        Results = new ObservableCollection<GlobalSearchResultViewModel>();
        HasNoResults = false;
    }

    /// <summary>Row activation for a repo result: the same selection a row press does
    /// (bar reveals, Overview tab loads, the row highlights).</summary>
    public void SelectRepo(Repo repo) => _bar.OpenForRepo(repo);

    /// <summary>Resolves the note hit's repository by matching the hit's path against
    /// each repo's notes root; null for notes of repos the app does not track.</summary>
    public Repo? ResolveRepoForHit(NotesSearchHit hit)
    {
        if (_storeRoot.Length == 0)
        {
            return null;
        }

        foreach (var repo in _repos.Repos)
        {
            var root = _notes.GetRepoNotesRoot(_storeRoot, repo.Name ?? string.Empty);
            if (hit.FullPath.StartsWith(root, StringComparison.OrdinalIgnoreCase)
                && (hit.FullPath.Length == root.Length
                    || hit.FullPath[root.Length] is '/' or '\\'))
            {
                return repo;
            }
        }

        return null;
    }
}
