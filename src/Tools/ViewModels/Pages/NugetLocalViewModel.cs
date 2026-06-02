using System.Text.RegularExpressions;
using Avalonia.Controls;
using Avalonia.Platform.Storage;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Configuration;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Nuget Local page.
/// </summary>
public partial class NugetLocalViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private FileSystemWatcher? _watcher;
    private NugetLocalSettings _nugetSettings = new();
    private DateTime _lastChanges = DateTime.Now;

    // Guard to avoid saving settings while initial values are being applied
    private bool _isInitializing;

    [ObservableProperty]
    private ObservableCollection<string> _fileList = new();

    [ObservableProperty]
    private string _watchFolder = string.Empty;

    [ObservableProperty]
    private string _computedCopyFolder = string.Empty;

    [ObservableProperty]
    private string _globalPackagesFolder = string.Empty;

    [ObservableProperty]
    private bool _watchStarted;

    [ObservableProperty]
    private int _count;

    /// <summary>
    /// Gets the command to start/stop watching for changes.
    /// </summary>
    public IRelayCommand<object> WatchChangesCommand { get; }

    /// <summary>
    /// Gets the command to select folders.
    /// </summary>
    public IAsyncRelayCommand<string> SelectFolderCommand { get; }

    /// <summary>
    /// Gets the command to register the watch folder as a NuGet source.
    /// </summary>
    public IAsyncRelayCommand RegisterSourceCommand { get; }

    public NugetLocalViewModel(ISettingsService settingsService)
    {
        _settingsService = settingsService;

        WatchChangesCommand = new RelayCommand<object>(OnWatchChanges);
        SelectFolderCommand = new AsyncRelayCommand<string>(OnSelectFolderAsync);
        RegisterSourceCommand = new AsyncRelayCommand(OnRegisterSourceAsync);

        _ = OnInitializeAsync();
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        _isInitializing = true;
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            _nugetSettings = settings.NugetLocal ?? new NugetLocalSettings();

            WatchFolder = _nugetSettings.WatchFolder ?? string.Empty;
            ComputedCopyFolder = ComputeCopyFolder(WatchFolder);
            GlobalPackagesFolder = await GetGlobalPackagesFolderAsync();
        }
        finally
        {
            _isInitializing = false;
        }
    }

    partial void OnWatchFolderChanged(string value)
    {
        if (_isInitializing) return;
        _ = SaveSettingAsync(() => _nugetSettings.WatchFolder = value);
        ComputedCopyFolder = ComputeCopyFolder(value);
    }

    private string ComputeCopyFolder(string watchFolder)
    {
        if (string.IsNullOrWhiteSpace(watchFolder)) return string.Empty;
        try
        {
            return Path.Combine(watchFolder.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar), "nugets");
        }
        catch
        {
            return string.Empty;
        }
    }


    private async Task SaveSettingAsync(Action updateAction)
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            updateAction();
            await _settingsService.SaveSettingsAsync(settings);
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Error saving settings: {ex.Message}");
        }
    }

    private async Task OnSelectFolderAsync(string? operation)
    {
        try
        {
            var topLevel = TopLevel.GetTopLevel(App.MainWindow);
            if (topLevel == null)
            {
                ShowError("Could not get top-level window.");
                return;
            }

            var folders = await topLevel.StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
            {
                Title = "Select Watch Folder",
                AllowMultiple = false
            });

            if (folders.Count > 0)
            {
                var path = folders[0].Path.LocalPath;
                UpdateFolderPath(operation, path);
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Avalonia FolderPicker failed: {ex.Message}");
            ShowError($"Failed to open folder picker: {ex.Message}");
        }
    }

    private void UpdateFolderPath(string? operation, string path)
    {
        if (operation == "Watch")
        {
            WatchFolder = path;
        }
    }

    private void OnWatchChanges(object? started)
    {
        if (started is bool isStarted && isStarted)
        {
            if (string.IsNullOrEmpty(WatchFolder) || !Directory.Exists(WatchFolder))
            {
                ShowError("Watch folder path is invalid or not set in settings.");
                WatchStarted = false;
                return;
            }

            _watcher = new FileSystemWatcher(WatchFolder);
            _watcher.Created += FileCreated;
            _watcher.EnableRaisingEvents = true;
            _watcher.IncludeSubdirectories = true;
            _watcher.Filter = _nugetSettings.NugetPackageFilter ?? "*.nupkg";

            WatchStarted = true;
        }
        else if (_watcher != null)
        {
            _watcher.EnableRaisingEvents = false;
            _watcher.Dispose();
            _watcher = null;

            WatchStarted = false;
        }
    }

    private void FileCreated(object sender, FileSystemEventArgs e)
    {
        Avalonia.Threading.Dispatcher.UIThread.InvokeAsync(async () =>
        {
            try
            {
                // Always clear NuGet cache for this package
                await ClearPackageCacheAsync(e.FullPath);

                if (DateTime.Now < _lastChanges.AddSeconds(_nugetSettings.CountResetIntervalSeconds))
                {
                    Count++;
                }
                else
                {
                    Count = 1;
                    _lastChanges = DateTime.Now;
                }

                // Auto-copy package to computed folder (<WatchFolder>\nugets)
                try
                {
                    var targetDir = ComputedCopyFolder;
                    if (string.IsNullOrEmpty(targetDir))
                    {
                        FileList.Insert(0, $"  ⚠ Copy skipped: target folder not available");
                    }
                    else
                    {
                        Directory.CreateDirectory(targetDir);
                        var destPath = Path.Combine(targetDir, Path.GetFileName(e.FullPath));

                        var attempts = 0;
                        var copied = false;
                        while (attempts < 3 && !copied)
                        {
                            try
                            {
                                await Task.Delay(_nugetSettings.FileCopyDelayMs);
                                File.Copy(e.FullPath, destPath, true);
                                FileList.Insert(0, $"  ✓ Copied to {destPath}");
                                copied = true;
                            }
                            catch (Exception exCopy)
                            {
                                attempts++;
                                if (attempts >= 3)
                                {
                                    FileList.Insert(0, $"  ✗ Copy failed: {exCopy.Message}");
                                    Debug.WriteLine($"Copy failed for {e.FullPath} -> {destPath}: {exCopy.Message}");
                                }
                                else
                                {
                                    await Task.Delay(500);
                                }
                            }
                        }
                    }
                }
                catch (Exception exCopyOuter)
                {
                    Debug.WriteLine($"Auto-copy error: {exCopyOuter.Message}");
                    FileList.Insert(0, $"  ✗ Auto-copy error: {exCopyOuter.Message}");
                }
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"Error processing file {e.FullPath}: {ex.Message}");
                FileList.Insert(0, $"ERROR processing {e.FullPath}: {ex.Message}");
            }
        });
    }

    private async Task ClearPackageCacheAsync(string packagePath)
    {
        try
        {
            var (packageId, version) = ExtractPackageInfo(Path.GetFileName(packagePath));
            if (string.IsNullOrEmpty(packageId) || string.IsNullOrEmpty(version))
            {
                FileList.Insert(0, $"  ⚠ Could not parse package info from {Path.GetFileName(packagePath)}");
                return;
            }

            // Get global packages folder
            var globalPackagesPath = GlobalPackagesFolder;
            if (string.IsNullOrEmpty(globalPackagesPath))
            {
                FileList.Insert(0, "  ⚠ Could not locate NuGet global packages folder");
                return;
            }

            // Delete specific package version folder
            var packageFolderPath = Path.Combine(globalPackagesPath, packageId.ToLowerInvariant(), version.ToLowerInvariant());
            if (Directory.Exists(packageFolderPath))
            {
                Directory.Delete(packageFolderPath, true);
                FileList.Insert(0, $"  ✓ Cleared cache for {packageId} {version}");
                Debug.WriteLine($"Cleared cache: {packageFolderPath}");
            }
            else
            {
                FileList.Insert(0, $"  ℹ No cache found for {packageId} {version}");
            }
        }
        catch (Exception ex)
        {
            FileList.Insert(0, $"  ✗ Cache clear error: {ex.Message}");
            Debug.WriteLine($"Error clearing cache: {ex.Message}");
        }
    }

    private (string packageId, string version) ExtractPackageInfo(string fileName)
    {
        // Pattern: PackageId.Version.nupkg
        // Example: MyPackage.1.0.0.nupkg or MyPackage.1.0.0-beta.nupkg
        if (!fileName.EndsWith(".nupkg", StringComparison.OrdinalIgnoreCase))
        {
            return (string.Empty, string.Empty);
        }

        var nameWithoutExt = fileName.Substring(0, fileName.Length - 6); // Remove .nupkg

        const string pattern = @"^(?<id>.+?)\.(?<version>\d+(\.\d+)*(\-[^\+]+)?(\+[^\s]+)?)$";

        var match = Regex.Match(nameWithoutExt, pattern);
        if (match.Success)
        {
            var pkgId = match.Groups["id"].Value;
            var version = match.Groups["version"].Value;
            return (pkgId, version);
        }

        return (string.Empty, string.Empty);
    }

    private async Task<string> GetGlobalPackagesFolderAsync()
    {
        try
        {
            var startInfo = new System.Diagnostics.ProcessStartInfo
            {
                FileName = "dotnet",
                Arguments = "nuget locals global-packages --list",
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
                CreateNoWindow = true
            };

            using var process = Process.Start(startInfo);
            if (process == null)
            {
                return string.Empty;
            }

            var output = await process.StandardOutput.ReadToEndAsync();
            await process.WaitForExitAsync();

            // Output format: "global-packages: C:\Users\...\packages"
            var match = Regex.Match(output, @"global-packages:\s*(.+)");
            if (match.Success)
            {
                return match.Groups[1].Value.Trim();
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"Error getting global packages folder: {ex.Message}");
        }

        return string.Empty;
    }

    private async Task OnRegisterSourceAsync()
    {
        var folderToRegister = ComputedCopyFolder;
        if (string.IsNullOrEmpty(folderToRegister))
        {
            ShowError("Computed NuGet local cache folder is not available. Ensure a valid Watch Directory is set.");
            return;
        }

        // Ensure the folder exists (auto-create cache folder on register)
        try
        {
            Directory.CreateDirectory(folderToRegister);
            FileList.Insert(0, $"✓ Ensured cache folder exists: {folderToRegister}");
        }
        catch (Exception exCreate)
        {
            ShowError($"Failed to create cache folder: {exCreate.Message}");
            return;
        }

        try
        {
            var folderName = Path.GetFileName(folderToRegister.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar));
            var sourceName = $"{folderName}-local";

            var startInfo = new System.Diagnostics.ProcessStartInfo
            {
                FileName = "dotnet",
                Arguments = $"nuget add source \"{folderToRegister}\" --name \"{sourceName}\"",
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
                CreateNoWindow = true
            };

            using var process = Process.Start(startInfo);
            if (process == null) return;

            var output = await process.StandardOutput.ReadToEndAsync();
            var error = await process.StandardError.ReadToEndAsync();
            await process.WaitForExitAsync();

            if (process.ExitCode == 0)
            {
                ShowInfo("NuGet Source Registered", $"'{folderToRegister}' has been registered as '{sourceName}'.");
                FileList.Insert(0, $"✓ Registered NuGet source '{sourceName}': {folderToRegister}");
            }
            else if (output.Contains("already been added", StringComparison.OrdinalIgnoreCase))
            {
                ShowInfo("NuGet Source Already Registered", $"'{folderToRegister}' is already registered as '{sourceName}'.");
                FileList.Insert(0, $"ℹ NuGet source '{sourceName}' already registered: {folderToRegister}");
            }
            else
            {
                ShowError($"Failed to register source: {error}");
            }
        }
        catch (Exception ex)
        {
            ShowError($"Execution error: {ex.Message}");
        }
    }

    private void ShowInfo(string title, string message)
    {
        Debug.WriteLine($"[INFO] {title}: {message}");
    }

    private void ShowError(string message)
    {
        Debug.WriteLine($"[ERROR] {message}");
    }
}