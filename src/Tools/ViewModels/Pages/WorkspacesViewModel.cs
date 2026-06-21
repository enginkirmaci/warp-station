using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Workspaces page.
/// </summary>
public partial class WorkspacesViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly IDevToolsClient devToolsClient;
    private WorkspacesSettings _workspaceSettings = new();
    private readonly string _cacheFilePath;

    private static ObservableCollection<WorkspaceItem> _workspaces = new();
    private static ObservableCollection<WorkspaceItem> _platforms = new();

    [ObservableProperty]
    private string _filterText = string.Empty;

    [ObservableProperty]
    private ObservableCollection<WorkspaceItem> _filteredWorkspaces = new();

    [ObservableProperty]
    private ObservableCollection<WorkspaceItem> _filteredPlatforms = new();

    public WorkspacesViewModel(ISettingsService settingsService, IDevToolsClient devToolsClient)
    {
        _settingsService = settingsService;
        this.devToolsClient = devToolsClient;
        _ = OnInitializeAsync();
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        _workspaceSettings = settings.Workspaces ?? new WorkspacesSettings();

        // Load cache first if available
        if (!_workspaces.Any() && !_platforms.Any())
        {
            await LoadCacheAsync();
        }

        if (_workspaceSettings.WorkspaceScanFolders?.Any() == true)
        {
            // Always scan in background to keep data fresh, even if we loaded from cache
            _ = ScanWorkspacesAsync();
        }
        else
        {
            ApplyFilter();
        }
    }

    partial void OnFilterTextChanged(string value) => ApplyFilter();

    private async Task ScanWorkspacesAsync()
    {
        if (IsBusy) return;
        IsBusy = true;

        try
        {
            var workspaces = new List<WorkspaceItem>();
            var platforms = new List<WorkspaceItem>();
            var scanFolders = _workspaceSettings.WorkspaceScanFolders ?? Array.Empty<string>();
            var gitPattern = _workspaceSettings.GitFolderPattern ?? ".git";
            var platformPattern = _workspaceSettings.PlatformFolderName ?? "platform";
            var slnPattern = _workspaceSettings.SolutionFilePattern ?? "*.sln";

            await Task.Run(() =>
            {
                foreach (var folderPath in scanFolders)
                {
                    if (!Directory.Exists(folderPath)) continue;

                    var directories = GetAccessibleDirectoriesRecursively(folderPath, gitPattern);

                    foreach (var dir in directories)
                    {
                        var parentDir = Path.GetDirectoryName(dir);
                        if (parentDir == null) continue;

                        var solutionFiles = Directory.GetFiles(parentDir, slnPattern);
                        foreach (var solutionFile in solutionFiles)
                        {
                            workspaces.Add(new WorkspaceItem
                            {
                                SolutionName = Path.GetFileNameWithoutExtension(solutionFile),
                                FolderPath = Path.GetDirectoryName(solutionFile),
                                SolutionPath = solutionFile
                            });
                        }

                        if (dir.Contains(platformPattern, StringComparison.OrdinalIgnoreCase))
                        {
                            platforms.Add(new WorkspaceItem
                            {
                                PlatformName = Path.GetFileName(parentDir.TrimEnd(Path.DirectorySeparatorChar)),
                                FolderPath = parentDir
                            });
                        }
                    }
                }
            });

            // Update collections on UI thread
            Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(async () =>
            {
                _workspaces = new ObservableCollection<WorkspaceItem>(workspaces.DistinctBy(w => w.SolutionPath).OrderBy(w => w.SolutionName));
                _platforms = new ObservableCollection<WorkspaceItem>(platforms.DistinctBy(p => p.FolderPath).OrderBy(p => p.PlatformName));
                ApplyFilter();

                // Save to cache after scan
                await SaveCacheAsync();
            });
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Error scanning workspaces: {ex.Message}");
        }
        finally
        {
            IsBusy = false;
        }
    }

    private void ApplyFilter()
    {
        var filter = FilterText?.Trim();

        if (string.IsNullOrWhiteSpace(filter))
        {
            FilteredWorkspaces = new ObservableCollection<WorkspaceItem>(_workspaces);
            FilteredPlatforms = new ObservableCollection<WorkspaceItem>(_platforms);
        }
        else
        {
            FilteredWorkspaces = new ObservableCollection<WorkspaceItem>(
                _workspaces.Where(w => w.SolutionName?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true));
            FilteredPlatforms = new ObservableCollection<WorkspaceItem>(
                _platforms.Where(p => p.PlatformName?.Contains(filter, StringComparison.OrdinalIgnoreCase) == true));
        }
    }

    private IEnumerable<string> GetAccessibleDirectoriesRecursively(string rootPath, string searchPattern)
    {
        var foundDirectories = new List<string>();
        try
        {
            foreach (var dir in Directory.EnumerateDirectories(rootPath, searchPattern, SearchOption.TopDirectoryOnly))
            {
                foundDirectories.Add(dir);
            }

            var excludedFolders = _workspaceSettings?.ExcludedFolders ?? Array.Empty<string>();

            foreach (var subDir in Directory.EnumerateDirectories(rootPath))
            {
                var subDirName = Path.GetFileName(subDir);

                if (subDirName.Equals(searchPattern, StringComparison.OrdinalIgnoreCase) ||
                    excludedFolders.Any(excluded => excluded.Equals(subDir, StringComparison.OrdinalIgnoreCase)))
                {
                    continue;
                }

                foundDirectories.AddRange(GetAccessibleDirectoriesRecursively(subDir, searchPattern));
            }
        }
        catch (UnauthorizedAccessException)
        {
            Debug.WriteLine($"Access denied to folder: {rootPath}");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Error accessing folder {rootPath}: {ex.Message}");
        }

        return foundDirectories;
    }

    [RelayCommand]
    private void OpenSolution(string? solutionPath) => SafeStartProcess(solutionPath);

    [RelayCommand]
    private void OpenFolder(string? folderPath) => SafeStartProcess(folderPath);

    [RelayCommand]
    private void OpenWithVSCode(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        SafeStartProcess(_workspaceSettings?.VSCodeExecutable ?? "code", folderPath);
    }

    [RelayCommand]
    private void OpenWithTerminal(string? folderPath)
    {
        if (string.IsNullOrWhiteSpace(folderPath)) return;

        var exe = _workspaceSettings?.TerminalExecutable ?? "wt";
        var exeLower = exe.ToLowerInvariant();
        string args;
        if (exeLower.EndsWith("wt.exe") || exeLower == "wt")
            args = $"-d \"{folderPath}\"";
        else if (exeLower.Contains("powershell") || exeLower.Contains("pwsh"))
            args = $"-NoExit -Command \"Set-Location -LiteralPath '{folderPath}'\"";
        else if (exeLower.EndsWith("cmd.exe") || exeLower == "cmd")
            args = $"/k cd /d \"{folderPath}\"";
        else
            args = $"\"{folderPath}\"";

        SafeStartProcess(exe, args);
    }

    private void SafeStartProcess(string? fileName, string? arguments = null, bool hideWindow = false, bool runWithOutAdmin = false)
    {
        if (string.IsNullOrWhiteSpace(fileName)) return;

        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = fileName,
                Arguments = arguments ?? string.Empty,
                UseShellExecute = true,
                CreateNoWindow = hideWindow,
                WindowStyle = hideWindow ? ProcessWindowStyle.Hidden : ProcessWindowStyle.Normal
            });
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Failed to start process '{fileName}' with args '{arguments}': {ex.Message}");
        }
    }

    [RelayCommand]
    private async Task RefreshAsync()
    {
        _workspaces.Clear();
        _platforms.Clear();
        await OnInitializeAsync();
    }

    [RelayCommand]
    private async Task OpenSettingsAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var currentWorkspaceSettings = settings.Workspaces ?? new WorkspacesSettings();

            var dialog = new Tools.Views.Windows.WorkspaceSettingsDialog(currentWorkspaceSettings);
            await dialog.ShowDialog(App.MainWindow);

            settings.Workspaces = dialog.Settings;
            await _settingsService.SaveSettingsAsync(settings);

            _workspaceSettings = dialog.Settings;
            _workspaces.Clear();
            _platforms.Clear();
            await OnInitializeAsync();
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Error opening workspace settings: {ex.Message}");
        }
    }

    private async Task LoadCacheAsync()
    {
        try
        {
            if (File.Exists(_cacheFilePath))
            {
                var json = await File.ReadAllTextAsync(_cacheFilePath);
                var cache = System.Text.Json.JsonSerializer.Deserialize<WorkspaceCache>(json, new System.Text.Json.JsonSerializerOptions
                {
                    PropertyNameCaseInsensitive = true
                });

                if (cache != null)
                {
                    _workspaces = new ObservableCollection<WorkspaceItem>(cache.Workspaces ?? new List<WorkspaceItem>());
                    _platforms = new ObservableCollection<WorkspaceItem>(cache.Platforms ?? new List<WorkspaceItem>());
                    ApplyFilter();
                }
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Error loading workspace cache: {ex.Message}");
        }
    }

    private async Task SaveCacheAsync()
    {
        try
        {
            var cache = new WorkspaceCache
            {
                Workspaces = _workspaces.ToList(),
                Platforms = _platforms.ToList()
            };

            var json = System.Text.Json.JsonSerializer.Serialize(cache, new System.Text.Json.JsonSerializerOptions
            {
                WriteIndented = true
            });

            await File.WriteAllTextAsync(_cacheFilePath, json);
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Error saving workspace cache: {ex.Message}");
        }
    }
}