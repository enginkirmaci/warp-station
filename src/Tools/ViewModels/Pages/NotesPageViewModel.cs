using System.Collections.ObjectModel;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Helpers;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.ViewModels.Pages;

/// <summary>One node of the notes tree (a folder or a .md note). The whole tree is
/// rebuilt on reload, so nodes are plain identity + the observable dirty flag the tree
/// rows bind.</summary>
public sealed class NoteNodeViewModel : ObservableObject
{
    public NoteNodeViewModel(string name, string fullPath, bool isFolder, IEnumerable<NoteNodeViewModel>? children = null)
    {
        Name = name;
        FullPath = fullPath;
        IsFolder = isFolder;
        Children = children is null ? new() : new(children);
    }

    public string Name { get; }

    public string FullPath { get; }

    public bool IsFolder { get; }

    public ObservableCollection<NoteNodeViewModel> Children { get; }

    private bool _isDirty;

    /// <summary>The note has unsaved in-memory edits (the tree row's dirty dot).</summary>
    public bool IsDirty
    {
        get => _isDirty;
        set => SetProperty(ref _isDirty, value);
    }
}

/// <summary>
/// ViewModel for the dedicated Notes page: per-repository markdown notes under the
/// configured store path (one subfolder per repository). The tree is scoped to the
/// bottom bar's selected repo — the page reloads on every show, so repo switching
/// (which only happens on the Repositories table) is picked up on the next open.
/// Unsaved edits are held per note path in memory (the VM is a window-lifetime
/// singleton) and survive navigating away and back; Save writes through to disk.
/// </summary>
public partial class NotesPageViewModel : ObservableObject
{
    private readonly INotesService _notes;
    private readonly ISettingsService _settings;
    private readonly INotificationService _notifications;
    private readonly IProcessLauncher _processLauncher;
    private readonly BottomBarViewModel _bar;

    private const int SearchDebounceMs = 150;
    private const int DeleteArmTimeoutMs = 3000;

    private readonly UiDebounce _searchDebounce = new(SearchDebounceMs);
    private readonly UiDebounce _deleteArmDebounce = new(DeleteArmTimeoutMs);

    /// <summary>Stale-load guards for the async tree/note loads (a repo switch or a
    /// second open landing while an earlier read is in flight must be discarded).</summary>
    private int _treeLoadGeneration;
    private int _noteLoadGeneration;

    /// <summary>The selected repo's notes folder (resolved per navigation).</summary>
    private string _repoNotesRoot = string.Empty;

    /// <summary>The open note's on-disk content — the baseline the dirty flag compares
    /// against (value compare, not timing: programmatic writes must not mark dirty).</summary>
    private string _openNoteOnDisk = string.Empty;

    /// <summary>Unsaved note buffers by full path, kept across navigations.</summary>
    private readonly Dictionary<string, string> _unsaved = new(StringComparer.OrdinalIgnoreCase);

    public NotesPageViewModel(
        INotesService notes,
        ISettingsService settings,
        INotificationService notifications,
        IProcessLauncher processLauncher,
        BottomBarViewModel bar)
    {
        _notes = notes;
        _settings = settings;
        _notifications = notifications;
        _processLauncher = processLauncher;
        _bar = bar;
    }

    // ---- tree / selection ----
    [ObservableProperty]
    private ObservableCollection<NoteNodeViewModel> _tree = new();

    [ObservableProperty]
    private bool _hasNotes;

    [ObservableProperty]
    private NoteNodeViewModel? _selectedNode;

    // ---- open note / editor ----
    [ObservableProperty]
    private NoteNodeViewModel? _openNote;

    [ObservableProperty]
    private string _noteText = string.Empty;

    [ObservableProperty]
    private bool _isDirty;

    [ObservableProperty]
    private bool _isEditMode = true;

    // ---- search ----
    [ObservableProperty]
    private string _searchText = string.Empty;

    [ObservableProperty]
    private bool _isSearching;

    [ObservableProperty]
    private ObservableCollection<NotesSearchHit> _searchResults = new();

    // ---- creation / delete ----
    [ObservableProperty]
    private string _newNoteName = string.Empty;

    [ObservableProperty]
    private string _newFolderName = string.Empty;

    [ObservableProperty]
    private bool _isDeleteArmed;

    // ---- repo context mirrors (flat, raised in change hooks) ----
    [ObservableProperty]
    private string? _repoName;

    [ObservableProperty]
    private bool _hasRepo;

    public bool HasOpenNote => OpenNote is not null;

    public string OpenNoteName => OpenNote?.Name ?? string.Empty;

    public string DeleteTooltip => IsDeleteArmed ? "Click again to delete" : "Delete this note";

    partial void OnOpenNoteChanged(NoteNodeViewModel? value)
    {
        OnPropertyChanged(nameof(HasOpenNote));
        OnPropertyChanged(nameof(OpenNoteName));
    }

    partial void OnIsDeleteArmedChanged(bool value) => OnPropertyChanged(nameof(DeleteTooltip));

    /// <summary>Navigation load (fired on every page show): re-resolves the store from
    /// the latest settings, re-reads the bar's selected repo, rebuilds the tree and
    /// re-opens the previously open note when it still exists. Unsaved buffers survive.</summary>
    public async Task OnNavigatedToAsync()
    {
        var generation = ++_treeLoadGeneration;
        IsDeleteArmed = false;
        SearchText = string.Empty;
        SearchResults.Clear();
        IsEditMode = true;

        Repo? repo = _bar.SelectedRepo;
        RepoName = repo?.Name;
        HasRepo = repo is not null;

        if (repo?.Name is null)
        {
            OpenNote = null;
            NoteText = string.Empty;
            SelectedNode = null;
            Tree = new();
            HasNotes = false;
            return;
        }

        var settings = await _settings.GetSettingsAsync();
        if (generation != _treeLoadGeneration)
        {
            return;
        }

        var storeRoot = _notes.ResolveStoreRoot(settings.General?.NotesStorePath);
        _repoNotesRoot = _notes.GetRepoNotesRoot(storeRoot, repo.Name);

        await ReloadTreeAsync(generation, restoreOpenPath: OpenNote?.FullPath);
    }

    /// <summary>Rebuilds the tree from disk and re-selects the open note. Callers pass
    /// the generation they loaded under; a newer navigation supersedes this reload.</summary>
    private async Task ReloadTreeAsync(int generation, string? restoreOpenPath)
    {
        var items = await _notes.LoadTreeAsync(_repoNotesRoot);
        if (generation != _treeLoadGeneration)
        {
            return;
        }

        var tree = new ObservableCollection<NoteNodeViewModel>(items.Select(ToNode));
        Tree = tree;
        HasNotes = tree.Count > 0;

        NoteNodeViewModel? nodeToSelect = null;
        if (restoreOpenPath is not null)
        {
            nodeToSelect = FindNote(tree, restoreOpenPath);
        }

        // The ItemsSource swap echoes SelectedItem=null through the two-way selection
        // binding; the programmatic re-selection lands after that chain settles.
        Dispatcher.UIThread.Post(() =>
        {
            if (generation != _treeLoadGeneration)
            {
                return;
            }

            SelectedNode = nodeToSelect;
            if (nodeToSelect is null)
            {
                OpenNote = null;
                NoteText = string.Empty;
                IsDirty = false;
            }
        });
    }

    private static NoteNodeViewModel ToNode(NotesTreeItem item)
        => new(item.Name, item.FullPath, item.IsFolder, item.IsFolder ? item.Children.Select(ToNode) : null);

    private static NoteNodeViewModel? FindNote(ObservableCollection<NoteNodeViewModel> nodes, string fullPath)
    {
        foreach (var node in nodes)
        {
            if (!node.IsFolder && string.Equals(node.FullPath, fullPath, StringComparison.OrdinalIgnoreCase))
            {
                return node;
            }

            if (node.Children.Count > 0 && FindNote(node.Children, fullPath) is { } found)
            {
                return found;
            }
        }

        return null;
    }

    partial void OnSelectedNodeChanged(NoteNodeViewModel? value)
    {
        if (value is { IsFolder: false })
        {
            _ = OpenNoteCoreAsync(value);
        }
    }

    /// <summary>Opens a note from the tree: the in-memory unsaved buffer wins over disk.</summary>
    private async Task OpenNoteCoreAsync(NoteNodeViewModel node)
    {
        var generation = ++_noteLoadGeneration;
        OpenNote = node;
        IsDeleteArmed = false;

        if (_unsaved.TryGetValue(node.FullPath, out var buffered))
        {
            _openNoteOnDisk = await ReadDiskBaselineAsync(node.FullPath, generation) ?? string.Empty;
            NoteText = buffered;
            return;
        }

        var content = await ReadDiskBaselineAsync(node.FullPath, generation);
        if (content is null || generation != _noteLoadGeneration)
        {
            return;
        }

        _openNoteOnDisk = content;
        NoteText = content;
    }

    /// <summary>Reads the note from disk; null (and silence) when a newer load superseded
    /// this one, the note vanished, or it stopped being the open one meanwhile.</summary>
    private async Task<string?> ReadDiskBaselineAsync(string path, int generation)
    {
        try
        {
            var content = await _notes.ReadNoteAsync(path);
            if (generation != _noteLoadGeneration
                || !string.Equals(OpenNote?.FullPath, path, StringComparison.OrdinalIgnoreCase))
            {
                return null;
            }

            return content;
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to read note {Path}", path);
            _notifications.Show("Failed to read the note", NotificationKind.Error);
            return null;
        }
    }

    partial void OnNoteTextChanged(string value)
    {
        if (OpenNote is null)
        {
            return;
        }

        // Value-compare against the on-disk baseline so the programmatic load write is
        // not mistaken for a user edit (a timing guard cannot cover the echo).
        IsDirty = !string.Equals(value, _openNoteOnDisk, StringComparison.Ordinal);
        if (IsDirty)
        {
            _unsaved[OpenNote.FullPath] = value;
        }
        else
        {
            _unsaved.Remove(OpenNote.FullPath);
        }

        OpenNote.IsDirty = IsDirty;
    }

    [RelayCommand]
    private async Task SaveAsync()
    {
        if (OpenNote is null)
        {
            return;
        }

        try
        {
            await _notes.WriteNoteAsync(OpenNote.FullPath, NoteText);
            _openNoteOnDisk = NoteText;
            _unsaved.Remove(OpenNote.FullPath);
            IsDirty = false;
            OpenNote.IsDirty = false;
            _notifications.Show("Note saved", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to save note {Path}", OpenNote.FullPath);
            _notifications.Show("Failed to save the note", NotificationKind.Error);
        }
    }

    /// <summary>The folder new notes/folders land in: the selected folder, else the
    /// selected note's folder, else the repo's notes root.</summary>
    private string CurrentParentDirectory
        => SelectedNode is { } node
            ? node.IsFolder ? node.FullPath : Path.GetDirectoryName(node.FullPath)!
            : _repoNotesRoot;

    [RelayCommand]
    private async Task CreateNoteAsync()
    {
        if (!HasRepo)
        {
            _notifications.Show("Select a repository first", NotificationKind.Warning);
            return;
        }

        var name = _notes.SanitizeEntryName(NewNoteName, enforceMarkdownExtension: true);
        if (name is null)
        {
            _notifications.Show("Enter a note name", NotificationKind.Warning);
            return;
        }

        try
        {
            var path = await _notes.CreateNoteAsync(_repoNotesRoot, CurrentParentDirectory, name);
            NewNoteName = string.Empty;
            await ReloadTreeAsync(_treeLoadGeneration, restoreOpenPath: null);
            Dispatcher.UIThread.Post(() =>
            {
                if (FindNote(Tree, path) is { } node)
                {
                    SelectedNode = node;
                }
            });
        }
        catch (Exception ex) when (ex is IOException or ArgumentException or InvalidOperationException)
        {
            Log.Logger.Warning(ex, "Failed to create note {Name}", name);
            _notifications.Show(ex.Message, NotificationKind.Error);
        }
    }

    [RelayCommand]
    private async Task CreateFolderAsync()
    {
        if (!HasRepo)
        {
            _notifications.Show("Select a repository first", NotificationKind.Warning);
            return;
        }

        var name = _notes.SanitizeEntryName(NewFolderName, enforceMarkdownExtension: false);
        if (name is null)
        {
            _notifications.Show("Enter a folder name", NotificationKind.Warning);
            return;
        }

        try
        {
            await _notes.CreateFolderAsync(_repoNotesRoot, CurrentParentDirectory, name);
            NewFolderName = string.Empty;
            await ReloadTreeAsync(_treeLoadGeneration, restoreOpenPath: OpenNote?.FullPath);
        }
        catch (Exception ex) when (ex is IOException or ArgumentException or InvalidOperationException)
        {
            Log.Logger.Warning(ex, "Failed to create notes folder {Name}", name);
            _notifications.Show(ex.Message, NotificationKind.Error);
        }
    }

    /// <summary>Two-step delete: the first click arms the button (auto-disarmed after a
    /// short grace period), the second within it deletes the open note.</summary>
    [RelayCommand]
    private async Task DeleteOpenNoteAsync()
    {
        if (OpenNote is null)
        {
            return;
        }

        if (!IsDeleteArmed)
        {
            IsDeleteArmed = true;
            _deleteArmDebounce.Debounce(() => IsDeleteArmed = false);
            return;
        }

        _deleteArmDebounce.Cancel();
        IsDeleteArmed = false;
        var path = OpenNote.FullPath;
        try
        {
            await _notes.DeleteAsync(_repoNotesRoot, path);
            _unsaved.Remove(path);
            OpenNote = null;
            NoteText = string.Empty;
            IsDirty = false;
            await ReloadTreeAsync(_treeLoadGeneration, restoreOpenPath: null);
            _notifications.Show("Note deleted", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to delete note {Path}", path);
            _notifications.Show("Failed to delete the note", NotificationKind.Error);
        }
    }

    partial void OnSearchTextChanged(string value)
    {
        IsSearching = !string.IsNullOrWhiteSpace(value);
        if (!IsSearching)
        {
            _searchDebounce.Cancel();
            SearchResults.Clear();
            return;
        }

        _searchDebounce.Debounce(() => _ = RunSearchNow());
    }

    /// <summary>Immediate search (Enter); the debounced path reuses it.</summary>
    [RelayCommand]
    private async Task RunSearchNow()
    {
        var term = SearchText.Trim();
        if (term.Length == 0)
        {
            SearchResults.Clear();
            return;
        }

        var generation = _treeLoadGeneration;
        try
        {
            var hits = await _notes.SearchAsync(_repoNotesRoot, term);
            if (generation != _treeLoadGeneration)
            {
                return;
            }

            SearchResults = new ObservableCollection<NotesSearchHit>(hits);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Notes search failed for '{Term}'", term);
            _notifications.Show("Search failed", NotificationKind.Error);
        }
    }

    /// <summary>Opens a search hit. When the note is missing from the tree (deleted
    /// mid-search) a lightweight stand-in node still opens it.</summary>
    public async Task OpenSearchResultAsync(NotesSearchHit hit)
    {
        await OpenNoteCoreAsync(FindNote(Tree, hit.FullPath)
            ?? new NoteNodeViewModel(hit.Name, hit.FullPath, isFolder: false));
        IsEditMode = true;
    }

    [RelayCommand]
    private void ClearSearch() => SearchText = string.Empty;

    /// <summary>Opens the repo's notes folder (or the store root before the first note
    /// exists) in the OS file manager.</summary>
    [RelayCommand]
    private void OpenStoreFolder()
    {
        if (!HasRepo)
        {
            _notifications.Show("Select a repository first", NotificationKind.Warning);
            return;
        }

        try
        {
            var folder = Directory.Exists(_repoNotesRoot)
                ? _repoNotesRoot
                : Path.GetDirectoryName(_repoNotesRoot)!;
            Directory.CreateDirectory(folder);
            _processLauncher.StartProcess(folder);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to open the notes folder");
            _notifications.Show("Failed to open the notes folder", NotificationKind.Error);
        }
    }
}
