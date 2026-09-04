using System.Collections.ObjectModel;
using System.ComponentModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// One scanned repository row in the Add Repositories dialog. Repos already tracked by
/// the app are listed but locked ("Already added") so the dialog doubles as a scan
/// report; everything else is pre-checked for the user to deselect.
/// </summary>
public partial class FoundRepository : ObservableObject
{
    public string Name { get; }
    public string FolderPath { get; }

    /// <summary>Whether this repo is already in the tracked list (row locked, unchecked).</summary>
    public bool IsAlreadyTracked { get; }

    /// <summary>Locked rows cannot be checked; the CheckBox binds this.</summary>
    public bool CanSelect => !IsAlreadyTracked;

    [ObservableProperty]
    private bool _isSelected;

    public FoundRepository(Repo repo, bool isAlreadyTracked)
    {
        Name = repo.Name ?? string.Empty;
        FolderPath = repo.FolderPath ?? string.Empty;
        IsAlreadyTracked = isAlreadyTracked;
        _isSelected = !isAlreadyTracked;
    }
}

/// <summary>
/// ViewModel for the <see cref="Views.Windows.AddRepositoryDialog"/>. Holds the folder
/// to scan, runs the scan through <see cref="IRepoScanner"/> (reusing the repo scan
/// settings' exclusions/patterns/depth), and presents the discovered repositories for
/// selection. The dialog returns the selected folder paths; merging them into the
/// persisted scan folders is the caller's job.
/// </summary>
public partial class AddRepositoryViewModel : ObservableObject
{
    private readonly IRepoScanner _scanner;
    private readonly ReposSettings _settings;
    private readonly HashSet<string> _trackedPaths;

    [ObservableProperty]
    private string _folderPath = string.Empty;

    [ObservableProperty]
    private bool _isScanning;

    [ObservableProperty]
    private bool _hasScanned;

    /// <summary>
    /// Header checkbox state over the selectable rows: all checked, none checked, or
    /// null (indeterminate) for a mix. Clicks only ever deliver true/false.
    /// </summary>
    [ObservableProperty]
    private bool? _isAllSelected;

    /// <summary>Whether any scanned row is selectable (drives the header checkbox).</summary>
    public bool HasSelectable => FoundRepos.Any(r => r.CanSelect);

    /// <summary>
    /// Scan state line: idle hint, folder-not-found, scan-failure, or the result summary
    /// ("3 repositories found · 1 already added").
    /// </summary>
    [ObservableProperty]
    private string _statusText = "Pick or type a folder, then scan it for git repositories.";

    public ObservableCollection<FoundRepository> FoundRepos { get; } = new();

    /// <summary>Whether the Add action has anything to do (drives the Add button).</summary>
    public bool HasSelection => SelectedCount > 0;

    /// <summary>How many checked rows will be added.</summary>
    public int SelectedCount => FoundRepos.Count(r => r.IsSelected && r.CanSelect);

    /// <summary>The Add button label, counting what will be added ("Add 3 repositories").</summary>
    public string AddButtonText => HasSelection
        ? $"Add {SelectedCount} {(SelectedCount == 1 ? "repository" : "repositories")}"
        : "Add";

    public AddRepositoryViewModel(
        ReposSettings settings,
        IEnumerable<Repo> trackedRepos,
        IRepoScanner scanner)
    {
        _settings = settings;
        _scanner = scanner;
        _trackedPaths = new HashSet<string>(
            trackedRepos.Where(r => r.FolderPath is not null).Select(r => r.FolderPath!),
            RepoPath.Comparer);
    }

    partial void OnFolderPathChanged(string value) => ScanCommand.NotifyCanExecuteChanged();

    private bool _updatingAllSelection;

    /// <summary>
    /// Header checkbox click: check/uncheck every selectable row, leaving the locked
    /// "Already added" rows untouched. Guarded so the per-item echo back through
    /// <see cref="RaiseSelectionBindings"/> cannot re-enter.
    /// </summary>
    partial void OnIsAllSelectedChanged(bool? value)
    {
        if (_updatingAllSelection || value is null)
            return;

        _updatingAllSelection = true;
        try
        {
            foreach (var item in FoundRepos)
                if (item.CanSelect)
                    item.IsSelected = value.Value;
            RaiseSelectionBindings();
        }
        finally
        {
            _updatingAllSelection = false;
        }
    }

    private bool CanScan() => !IsScanning && !string.IsNullOrWhiteSpace(FolderPath);

    /// <summary>
    /// Scans the entered folder for git repositories. The scan uses a throwaway settings
    /// instance rooted at the entered path but carrying the persisted exclusions, folder
    /// pattern and depth, so the dialog discovers exactly what a regular scan rooted
    /// there would.
    /// </summary>
    [RelayCommand(CanExecute = nameof(CanScan))]
    private async Task ScanAsync()
    {
        var path = FolderPath.Trim();
        if (!Directory.Exists(path))
        {
            FoundRepos.Clear();
            HasScanned = false;
            RaiseSelectionBindings();
            StatusText = "Folder does not exist.";
            return;
        }

        IsScanning = true;
        ScanCommand.NotifyCanExecuteChanged();
        try
        {
            var scanSettings = new ReposSettings
            {
                RepoScanFolders = new[] { path },
                ExcludedFolders = _settings.ExcludedFolders,
                GitFolderPattern = _settings.GitFolderPattern,
                SolutionFilePattern = _settings.SolutionFilePattern,
                PlatformFolderName = _settings.PlatformFolderName,
                MaxScanDepth = _settings.MaxScanDepth
            };
            var result = await _scanner.ScanAsync(scanSettings);
            SetResults(result.Repos);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Add Repositories: scan failed for {Path}", path);
            FoundRepos.Clear();
            HasScanned = false;
            RaiseSelectionBindings();
            StatusText = "Scan failed — see the log for details.";
        }
        finally
        {
            IsScanning = false;
            ScanCommand.NotifyCanExecuteChanged();
        }
    }

    private void SetResults(IReadOnlyList<Repo> repos)
    {
        foreach (var item in FoundRepos)
            item.PropertyChanged -= OnItemPropertyChanged;
        FoundRepos.Clear();

        foreach (var repo in repos.OrderBy(r => r.Name, StringComparer.OrdinalIgnoreCase))
        {
            var tracked = repo.FolderPath is not null && _trackedPaths.Contains(repo.FolderPath);
            var item = new FoundRepository(repo, tracked);
            item.PropertyChanged += OnItemPropertyChanged;
            FoundRepos.Add(item);
        }

        HasScanned = true;
        StatusText = repos.Count == 0
            ? "No git repositories found under this folder."
            : BuildSummary();
        RaiseSelectionBindings();
    }

    private string BuildSummary()
    {
        var fresh = FoundRepos.Count(r => r.CanSelect);
        var tracked = FoundRepos.Count - fresh;
        return tracked > 0
            ? $"{FoundRepos.Count} {(FoundRepos.Count == 1 ? "repository" : "repositories")} found · {tracked} already added"
            : $"{FoundRepos.Count} {(FoundRepos.Count == 1 ? "repository" : "repositories")} found";
    }

    private void OnItemPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(FoundRepository.IsSelected))
            RaiseSelectionBindings();
    }

    /// <summary>Raise selection-driven bindings after a check change or list rebuild.</summary>
    private void RaiseSelectionBindings()
    {
        OnPropertyChanged(nameof(HasSelection));
        OnPropertyChanged(nameof(SelectedCount));
        OnPropertyChanged(nameof(AddButtonText));
        OnPropertyChanged(nameof(HasSelectable));
        UpdateAllSelectedFlag();
    }

    /// <summary>
    /// Syncs the header checkbox with the selectable rows: all checked, none, or
    /// indeterminate for a mix; false when there is nothing selectable at all.
    /// </summary>
    private void UpdateAllSelectedFlag()
    {
        var selectable = FoundRepos.Where(r => r.CanSelect).ToList();
        var selected = selectable.Count(r => r.IsSelected);
        bool? state = selectable.Count == 0 || selected == 0
            ? false
            : selected == selectable.Count ? true : null;
        if (EqualityComparer<bool?>.Default.Equals(IsAllSelected, state))
            return;
        _updatingAllSelection = true;
        try
        {
            IsAllSelected = state;
        }
        finally
        {
            _updatingAllSelection = false;
        }
    }

    /// <summary>The checked, not-yet-tracked repo paths to add.</summary>
    public IReadOnlyList<string> GetSelectedPaths()
        => FoundRepos
            .Where(r => r.IsSelected && r.CanSelect && !string.IsNullOrEmpty(r.FolderPath))
            .Select(r => r.FolderPath)
            .ToList();
}
