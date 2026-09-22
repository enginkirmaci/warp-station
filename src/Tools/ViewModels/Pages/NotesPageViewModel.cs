using System.Collections.ObjectModel;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using MarkdownViewerKit;
using Serilog;
using Tools.Helpers;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.ViewModels.Pages;

/// <summary>Layout mode of the note pane: source editor only, rendered note with
/// click-to-edit blocks, editor+preview side by side, or preview only.</summary>
public enum NoteEditorMode
{
    Edit,
    Live,
    Split,
    Preview,
}

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

    private bool _isExpanded;

    /// <summary>Folder expansion state (the tree's item style binds it two-way so
    /// expansion survives tree rebuilds).</summary>
    public bool IsExpanded
    {
        get => _isExpanded;
        set => SetProperty(ref _isExpanded, value);
    }
}

/// <summary>
/// ViewModel for the dedicated Notes page: markdown notes under the configured store
/// path (one subfolder per repository). The title-bar note button opens the whole
/// store — every repository's notes folder at the root; a repo row's note button on
/// the Repositories table and a global-search note hit open the page scoped to that
/// note's repo folder instead (tree, search and new notes stay inside it). The page
/// reloads on every show, so repo switching is picked up on the next open. Unsaved
/// edits are held per note path in memory (the VM is a window-lifetime singleton) and
/// survive navigating away and back; Save writes through to disk.
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

    /// <summary>The note a global-search activation wants open, consumed by the next
    /// navigation's tree restore (selection rides the regular restore path).</summary>
    private string? _pendingOpenPath;

    /// <summary>The notes store root (resolved per navigation): the tree's scope.</summary>
    private string _storeRoot = string.Empty;

    /// <summary>True on a repo-scoped entry (the table's note button, a global-search
    /// note hit): the tree, search and mutations scope to the selected repo's notes
    /// folder instead of the whole store. Cleared by the title-bar/whole-store
    /// entries.</summary>
    private bool _isRepoScoped;

    /// <summary>The root the tree, search and mutations anchor to on this show.</summary>
    private string ActiveRoot
        => _isRepoScoped && _repoNotesRoot.Length > 0 ? _repoNotesRoot : _storeRoot;

    /// <summary>The selected repo's notes folder under the store root (empty when no
    /// repo is selected) — the auto-expanded folder and the fallback target for new
    /// notes.</summary>
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
    private NoteEditorMode _editorMode = NoteEditorMode.Edit;

    // ---- status bar ----
    [ObservableProperty]
    private string _statusStats = "0 words · 0 chars · 1 line";

    [ObservableProperty]
    private string _cursorPosition = "Ln 1, Col 1";

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

    public string SearchPlaceholder => _isRepoScoped
        ? "Search this repo's notes…"
        : "Search all notes…";

    public string EmptyStateHint => _isRepoScoped
        ? "This repository has no notes yet — create one with +."
        : "Select a repository in the table, then create one with +.";

    /// <summary>Preview rendering is driven from the MarkdownViewerKit preview: task
    /// checkboxes become live controls, so toggling writes back through here.</summary>
    public void ToggleTaskAtLine(int line)
    {
        if (MarkdownTasks.Toggle(NoteText, line) is not { } next || next == NoteText)
        {
            return;
        }

        NoteText = next;
    }

    /// <summary>Splices a char-range replacement into the note text (the live mode's
    /// block editor commits through here); dirty flag, buffer and stats ride
    /// <see cref="OnNoteTextChanged"/>.</summary>
    public void ReplaceTextRange(int start, int end, string text)
        => NoteText = NoteText[..start] + text + NoteText[end..];

    // Segmented Edit/Live/Split/Preview pair: each ToggleButton binds its own flag two-way.
    public bool IsEditMode
    {
        get => EditorMode == NoteEditorMode.Edit;
        set
        {
            if (value)
            {
                EditorMode = NoteEditorMode.Edit;
            }
        }
    }

    public bool IsLiveMode
    {
        get => EditorMode == NoteEditorMode.Live;
        set
        {
            if (value)
            {
                EditorMode = NoteEditorMode.Live;
            }
        }
    }

    public bool IsSplitMode
    {
        get => EditorMode == NoteEditorMode.Split;
        set
        {
            if (value)
            {
                EditorMode = NoteEditorMode.Split;
            }
        }
    }

    public bool IsPreviewMode
    {
        get => EditorMode == NoteEditorMode.Preview;
        set
        {
            if (value)
            {
                EditorMode = NoteEditorMode.Preview;
            }
        }
    }

    public bool IsEditorVisible => EditorMode is NoteEditorMode.Edit or NoteEditorMode.Split;

    public bool IsPreviewVisible => EditorMode is NoteEditorMode.Split or NoteEditorMode.Preview;

    partial void OnEditorModeChanged(NoteEditorMode value)
    {
        OnPropertyChanged(nameof(IsEditMode));
        OnPropertyChanged(nameof(IsLiveMode));
        OnPropertyChanged(nameof(IsSplitMode));
        OnPropertyChanged(nameof(IsPreviewMode));
        OnPropertyChanged(nameof(IsEditorVisible));
        OnPropertyChanged(nameof(IsPreviewVisible));
    }

    partial void OnOpenNoteChanged(NoteNodeViewModel? value)
    {
        OnPropertyChanged(nameof(HasOpenNote));
        OnPropertyChanged(nameof(OpenNoteName));
    }

    partial void OnIsDeleteArmedChanged(bool value) => OnPropertyChanged(nameof(DeleteTooltip));

    /// <summary>Navigation load (fired on every page show from the title-bar note
    /// button): re-resolves the store from the latest settings, re-reads the
    /// bar's selected repo, rebuilds the whole-store tree and re-opens the previously
    /// open note when it still exists. Unsaved buffers survive.</summary>
    public Task OnNavigatedToAsync()
    {
        _isRepoScoped = false;
        return OnNavigatedToCoreAsync();
    }

    /// <summary>Row entry from the Repositories table's note button: the bar has by
    /// now selected the clicked repo, and the load scopes to its notes folder.</summary>
    public Task OnRepoNotesRequestedAsync()
    {
        _isRepoScoped = true;
        return OnNavigatedToCoreAsync();
    }

    private async Task OnNavigatedToCoreAsync()
    {
        var generation = ++_treeLoadGeneration;
        var pendingOpenPath = _pendingOpenPath;
        _pendingOpenPath = null;
        IsDeleteArmed = false;
        SearchText = string.Empty;
        SearchResults.Clear();
        EditorMode = NoteEditorMode.Edit;

        OnPropertyChanged(nameof(SearchPlaceholder));
        OnPropertyChanged(nameof(EmptyStateHint));

        Repo? repo = _bar.SelectedRepo;
        RepoName = repo?.Name;
        HasRepo = repo is not null;

        var settings = await _settings.GetSettingsAsync();
        if (generation != _treeLoadGeneration)
        {
            return;
        }

        _storeRoot = _notes.ResolveStoreRoot(settings.General?.NotesStorePath);
        _repoNotesRoot = repo?.Name is { } name
            ? _notes.GetRepoNotesRoot(_storeRoot, name)
            : string.Empty;

        await ReloadTreeAsync(generation, restoreOpenPath: pendingOpenPath ?? OpenNote?.FullPath);
    }

    /// <summary>Shows the page with a specific note open (the title-bar global search):
    /// the load scopes to the note's own repo — the caller resolves it and has by then
    /// selected it in the bar; null for a repo the app does not track falls back to the
    /// whole-store view. The pending path rides the regular navigation, whose tree
    /// restore selects and opens it. A note missing from the tree (deleted since the
    /// search) just opens the page.</summary>
    public async Task NavigateToNoteAsync(NotesSearchHit hit, Repo? repo)
    {
        _isRepoScoped = repo is not null;
        _pendingOpenPath = hit.FullPath;
        await OnNavigatedToCoreAsync();
    }

    /// <summary>Rebuilds the tree from disk and re-selects the open note. Callers pass
    /// the generation they loaded under; a newer navigation supersedes this reload.
    /// Expansion survives the rebuild (currently open folders keep their state); the
    /// selected repo's folder and the restored note's ancestors open on top.</summary>
    private async Task ReloadTreeAsync(int generation, string? restoreOpenPath)
    {
        var expanded = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        CollectExpanded(Tree, expanded);

        var items = await _notes.LoadTreeAsync(ActiveRoot);
        if (generation != _treeLoadGeneration)
        {
            return;
        }

        var tree = new ObservableCollection<NoteNodeViewModel>(items.Select(ToNode));
        ApplyExpanded(tree, expanded);

        if (HasRepo && FindNodePath(tree, _repoNotesRoot) is { } repoPath)
        {
            foreach (var node in repoPath)
            {
                node.IsExpanded = true;
            }
        }

        List<NoteNodeViewModel>? restored = null;
        if (restoreOpenPath is not null)
        {
            restored = FindNodePath(tree, restoreOpenPath);
            if (restored is not null)
            {
                for (var i = 0; i < restored.Count - 1; i++)
                {
                    restored[i].IsExpanded = true;
                }
            }
        }

        Tree = tree;
        HasNotes = tree.Count > 0;

        // The ItemsSource swap echoes SelectedItem=null through the two-way selection
        // binding; the programmatic re-selection lands after that chain settles.
        Dispatcher.UIThread.Post(() =>
        {
            if (generation != _treeLoadGeneration)
            {
                return;
            }

            SelectedNode = restored is { Count: > 0 } ? restored[^1] : null;
            if (restored is not { Count: > 0 })
            {
                OpenNote = null;
                NoteText = string.Empty;
                IsDirty = false;
            }
        });
    }

    private static void CollectExpanded(IEnumerable<NoteNodeViewModel> nodes, ISet<string> into)
    {
        foreach (var node in nodes)
        {
            if (!node.IsFolder)
            {
                continue;
            }

            if (node.IsExpanded)
            {
                into.Add(node.FullPath);
            }

            if (node.Children.Count > 0)
            {
                CollectExpanded(node.Children, into);
            }
        }
    }

    private static void ApplyExpanded(IEnumerable<NoteNodeViewModel> nodes, ISet<string> expanded)
    {
        foreach (var node in nodes)
        {
            if (!node.IsFolder)
            {
                continue;
            }

            node.IsExpanded = expanded.Contains(node.FullPath);
            if (node.Children.Count > 0)
            {
                ApplyExpanded(node.Children, expanded);
            }
        }
    }

    private static NoteNodeViewModel ToNode(NotesTreeItem item)
        => new(item.Name, item.FullPath, item.IsFolder, item.IsFolder ? item.Children.Select(ToNode) : null);

    /// <summary>Finds a node by full path and returns the chain from the tree root to
    /// it (the node itself last); null when absent. Folder paths resolve too (the
    /// selected repo's folder).</summary>
    private static List<NoteNodeViewModel>? FindNodePath(
        ObservableCollection<NoteNodeViewModel> nodes, string fullPath)
    {
        foreach (var node in nodes)
        {
            if (string.Equals(node.FullPath, fullPath, StringComparison.OrdinalIgnoreCase))
            {
                return new List<NoteNodeViewModel> { node };
            }

            if (node.IsFolder && node.Children.Count > 0 && FindNodePath(node.Children, fullPath) is { } found)
            {
                found.Insert(0, node);
                return found;
            }
        }

        return null;
    }

    partial void OnSelectedNodeChanged(NoteNodeViewModel? value)
    {
        // Selecting a folder row (label or chevron) opens it; collapse stays on the chevron.
        if (value is { IsFolder: true })
        {
            value.IsExpanded = true;
            return;
        }

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

        StatusStats = $"{CountWords(value)} words · {value.Length} chars · {1 + CountNewlines(value)} lines";
    }

    /// <summary>Caret readout for the status bar; the code-behind feeds SelectionChanged.</summary>
    public void UpdateCursorPosition(int caret)
    {
        caret = Math.Clamp(caret, 0, NoteText.Length);
        var upToCaret = NoteText[..caret];
        var lastNewline = upToCaret.LastIndexOf('\n');
        CursorPosition = $"Ln {1 + CountNewlines(upToCaret)}, Col {caret - lastNewline}";
    }

    private static int CountNewlines(string text)
    {
        var count = 0;
        foreach (var ch in text)
        {
            if (ch == '\n')
            {
                count++;
            }
        }

        return count;
    }

    private static int CountWords(string text)
    {
        var count = 0;
        var inWord = false;
        foreach (var ch in text)
        {
            if (char.IsWhiteSpace(ch))
            {
                inWord = false;
            }
            else if (!inWord)
            {
                inWord = true;
                count++;
            }
        }

        return count;
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
    /// selected note's folder, else the selected repo's notes root; null when nothing
    /// is selected anywhere (the create commands ask for a target).</summary>
    private string? CurrentParentDirectory
        => SelectedNode is { } node
            ? node.IsFolder ? node.FullPath : Path.GetDirectoryName(node.FullPath)!
            : HasRepo ? _repoNotesRoot
            : null;

    [RelayCommand]
    private async Task CreateNoteAsync()
    {
        if (CurrentParentDirectory is not { } parent)
        {
            _notifications.Show("Select a repository or a folder first", NotificationKind.Warning);
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
            var path = await _notes.CreateNoteAsync(ActiveRoot, parent, name);
            NewNoteName = string.Empty;
            await ReloadTreeAsync(_treeLoadGeneration, restoreOpenPath: null);
            Dispatcher.UIThread.Post(() =>
            {
                if (FindNodePath(Tree, path) is { } chain)
                {
                    for (var i = 0; i < chain.Count - 1; i++)
                    {
                        chain[i].IsExpanded = true;
                    }

                    SelectedNode = chain[^1];
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
        if (CurrentParentDirectory is not { } parent)
        {
            _notifications.Show("Select a repository or a folder first", NotificationKind.Warning);
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
            await _notes.CreateFolderAsync(ActiveRoot, parent, name);
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
            await _notes.DeleteAsync(ActiveRoot, path);
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
            var hits = await _notes.SearchAsync(ActiveRoot, term);
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
    /// mid-search) a lightweight stand-in node still opens it; otherwise its ancestors
    /// expand so the opened note stays visible.</summary>
    public async Task OpenSearchResultAsync(NotesSearchHit hit)
    {
        var chain = FindNodePath(Tree, hit.FullPath);
        if (chain is not null)
        {
            for (var i = 0; i < chain.Count - 1; i++)
            {
                chain[i].IsExpanded = true;
            }
        }

        await OpenNoteCoreAsync(chain is { Count: > 0 }
            ? chain[^1]
            : new NoteNodeViewModel(hit.Name, hit.FullPath, isFolder: false));
        EditorMode = NoteEditorMode.Edit;
    }

    [RelayCommand]
    private void ClearSearch() => SearchText = string.Empty;

    /// <summary>Opens the current notes folder — the selected folder / selected repo's
    /// folder, else the store root — in the OS file manager.</summary>
    [RelayCommand]
    private void OpenStoreFolder()
    {
        try
        {
            var folder = CurrentParentDirectory;
            if (string.IsNullOrEmpty(folder))
            {
                folder = _storeRoot;
            }

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
