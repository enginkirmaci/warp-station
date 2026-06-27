using System.Collections;
using System.Collections.ObjectModel;
using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text.RegularExpressions;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Long-running singleton service that watches a folder for new NuGet packages,
/// copies them to a local cache folder, and clears the matching NuGet global
/// package cache so consumer projects always fetch fresh DLLs.
/// </summary>
/// <remarks>
/// Thread safety: <see cref="FileSystemWatcher.Created"/> fires on threadpool threads,
/// so all mutations of shared state (the activity log, counters, flags) are serialized
/// through <see cref="_stateLock"/>. The activity log is a thread-safe snapshot source so
/// the UI thread can enumerate it safely. In-flight file-processing tasks are tracked so
/// shutdown can drain them, and a <see cref="CancellationToken"/> stops them promptly.
/// </remarks>
public class NugetLocalService : INugetLocalService, IDisposable
{
    private readonly ISettingsService _settingsService;
    private FileSystemWatcher? _watcher;
    private NugetLocalSettings _nugetSettings = new();
    private DateTime _lastChanges = DateTime.Now;
    private bool _isInitializing;
    private bool _disposed;
    private readonly Task _initializationTask;

    /// <summary>
    /// Serializes access to all mutable shared state below. The FileSystemWatcher
    /// callback runs on a threadpool thread while the UI reads these from the UI thread.
    /// </summary>
    private readonly object _stateLock = new();

    /// <summary>Tracks in-flight file-processing tasks so shutdown can drain them.</summary>
    private readonly List<Task> _pendingTasks = new();

    /// <summary>Cancels in-flight file-processing tasks on stop/dispose.</summary>
    private CancellationTokenSource _cts = new();

    // Bound/copyable snapshots so views can read them without referencing the service.
    public string WatchFolder { get; private set; } = string.Empty;
    public string ComputedCopyFolder { get; private set; } = string.Empty;
    public string GlobalPackagesFolder { get; private set; } = string.Empty;
    public bool IsWatching { get; private set; }
    public int Count { get; private set; }

    // Backing collection is only ever mutated under _stateLock; a snapshot is handed out
    // for safe cross-thread enumeration.
    private readonly ObservableCollection<string> _activityLog = new();
    public IReadOnlyList<string> ActivityLog
    {
        get
        {
            lock (_stateLock)
            {
                return _activityLog.ToList();
            }
        }
    }

    public event EventHandler? StateChanged;

    public NugetLocalService(ISettingsService settingsService)
    {
        _settingsService = settingsService;
        _initializationTask = InitializeAsync();
    }

    private async Task InitializeAsync()
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
            RaiseStateChanged();
        }
    }

    public async Task<NugetLocalSettings> GetSettingsAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        return settings.NugetLocal ?? new NugetLocalSettings();
    }

    public async Task SetWatchFolderAsync(string? path)
    {
        if (_isInitializing) return;
        path ??= string.Empty;

        WatchFolder = path;
        ComputedCopyFolder = ComputeCopyFolder(path);

        var settings = await _settingsService.GetSettingsAsync();
        settings.NugetLocal ??= new NugetLocalSettings();
        settings.NugetLocal.WatchFolder = path;
        await _settingsService.SaveSettingsAsync(settings);
        _nugetSettings = settings.NugetLocal;

        // If the watch was running, restart against the new folder.
        if (IsWatching)
        {
            Stop();
            _ = StartAsync();
        }
        else
        {
            RaiseStateChanged();
        }
    }

    public async Task<bool> StartAsync()
    {
        if (IsWatching) return true;

        // Ensure settings have been loaded before we try to start.
        await _initializationTask;

        if (string.IsNullOrEmpty(WatchFolder) || !Directory.Exists(WatchFolder))
        {
            AddLog("✗ Watch folder path is invalid or not set in settings.");
            RaiseStateChanged();
            return false;
        }

        await Task.Run(() =>
        {
            _watcher = new FileSystemWatcher(WatchFolder)
            {
                EnableRaisingEvents = true,
                IncludeSubdirectories = true,
                Filter = _nugetSettings.NugetPackageFilter ?? "*.nupkg"
            };
            _watcher.Created += FileCreated;
        });

        IsWatching = true;
        AddLog($"▶ Watching '{WatchFolder}' for new packages...");
        RaiseStateChanged();
        return true;
    }

    public void Stop()
    {
        CancellationTokenSource cts;
        List<Task> pending;

        lock (_stateLock)
        {
            if (_watcher == null)
            {
                IsWatching = false;
                pending = new List<Task>();
            }
            else
            {
                _watcher.EnableRaisingEvents = false;
                _watcher.Created -= FileCreated;
                _watcher.Dispose();
                _watcher = null;

                IsWatching = false;
                pending = _pendingTasks.ToList();
            }
            cts = _cts;
        }

        // Cancel in-flight work so it bails out at its next await, then drain the
        // (now-cancelling) tasks so we don't leave background work mutating state.
        cts.Cancel();
        DrainPending(pending);

        AddLog("⏹ Stopped watching.");
        RaiseStateChanged();
    }

    public async Task RegisterSourceAsync()
    {
        var folderToRegister = ComputedCopyFolder;
        if (string.IsNullOrEmpty(folderToRegister))
        {
            AddLog("✗ Computed NuGet local cache folder is not available. Ensure a valid Watch Directory is set.");
            RaiseStateChanged();
            return;
        }

        try
        {
            Directory.CreateDirectory(folderToRegister);
            AddLog($"✓ Ensured cache folder exists: {folderToRegister}");
        }
        catch (Exception exCreate)
        {
            AddLog($"✗ Failed to create cache folder: {exCreate.Message}");
            RaiseStateChanged();
            return;
        }

        try
        {
            var folderName = Path.GetFileName(folderToRegister.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar));
            var sourceName = $"{folderName}-local";

            var startInfo = new ProcessStartInfo
            {
                FileName = "dotnet",
                Arguments = $"nuget add source \"{folderToRegister}\" --name \"{sourceName}\"",
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false,
                CreateNoWindow = true
            };

            using var process = Process.Start(startInfo);
            if (process == null)
            {
                RaiseStateChanged();
                return;
            }

            var output = await process.StandardOutput.ReadToEndAsync();
            var error = await process.StandardError.ReadToEndAsync();
            await process.WaitForExitAsync();

            if (process.ExitCode == 0)
            {
                AddLog($"✓ Registered NuGet source '{sourceName}': {folderToRegister}");
            }
            else if (output.Contains("already been added", StringComparison.OrdinalIgnoreCase))
            {
                AddLog($"ℹ NuGet source '{sourceName}' already registered: {folderToRegister}");
            }
            else
            {
                AddLog($"✗ Failed to register source: {error}");
            }
        }
        catch (Exception ex)
        {
            AddLog($"✗ Execution error: {ex.Message}");
        }

        RaiseStateChanged();
    }

    private void FileCreated(object sender, FileSystemEventArgs e)
    {
        // FileSystemWatcher fires on a background thread; track the task so shutdown can
        // drain it, and observe the cancellation token so Stop/Dispose cancels it.
        Task task;
        lock (_stateLock)
        {
            task = ProcessCreatedFileAsync(e.FullPath, _cts.Token);
            _pendingTasks.Add(task);
        }

        // Remove from the pending list once complete (regardless of outcome).
        _ = task.ContinueWith(t =>
        {
            lock (_stateLock)
            {
                _pendingTasks.Remove(t);
            }
        }, TaskScheduler.Default);
    }

    private async Task ProcessCreatedFileAsync(string packagePath, CancellationToken cancellationToken)
    {
        try
        {
            await ClearPackageCacheAsync(packagePath);

            // Advance the activity window on every package so the counter reflects a
            // sliding window of recent activity, not a fixed window from the last reset.
            lock (_stateLock)
            {
                if (DateTime.Now < _lastChanges.AddSeconds(_nugetSettings.CountResetIntervalSeconds))
                {
                    Count++;
                    _lastChanges = DateTime.Now;
                }
                else
                {
                    Count = 1;
                    _lastChanges = DateTime.Now;
                }
            }

            var targetDir = ComputedCopyFolder;
            if (string.IsNullOrEmpty(targetDir))
            {
                AddLog("⚠ Copy skipped: target folder not available");
                return;
            }

            Directory.CreateDirectory(targetDir);
            var destPath = Path.Combine(targetDir, Path.GetFileName(packagePath));

            var attempts = 0;
            var copied = false;
            while (attempts < 3 && !copied)
            {
                cancellationToken.ThrowIfCancellationRequested();
                try
                {
                    await Task.Delay(_nugetSettings.FileCopyDelayMs, cancellationToken);
                    File.Copy(packagePath, destPath, true);
                    AddLog($"✓ Copied to {destPath}");
                    copied = true;
                }
                catch (OperationCanceledException)
                {
                    throw;
                }
                catch (Exception exCopy)
                {
                    attempts++;
                    if (attempts >= 3)
                    {
                        AddLog($"✗ Copy failed: {exCopy.Message}");
                    }
                    else
                    {
                        await Task.Delay(500, cancellationToken);
                    }
                }
            }
        }
        catch (OperationCanceledException)
        {
            // Shutdown in progress — expected, no log.
        }
        catch (Exception ex)
        {
            AddLog($"ERROR processing {packagePath}: {ex.Message}");
        }
        finally
        {
            RaiseStateChanged();
        }
    }

    private async Task ClearPackageCacheAsync(string packagePath)
    {
        var (packageId, version) = ExtractPackageInfo(Path.GetFileName(packagePath));
        if (string.IsNullOrEmpty(packageId) || string.IsNullOrEmpty(version))
        {
            AddLog($"⚠ Could not parse package info from {Path.GetFileName(packagePath)}");
            return;
        }

        var globalPackagesPath = GlobalPackagesFolder;
        if (string.IsNullOrEmpty(globalPackagesPath))
        {
            AddLog("⚠ Could not locate NuGet global packages folder");
            return;
        }

        var packageFolderPath = Path.Combine(globalPackagesPath, packageId.ToLowerInvariant(), version.ToLowerInvariant());
        if (Directory.Exists(packageFolderPath))
        {
            await Task.Run(() => Directory.Delete(packageFolderPath, true));
            AddLog($"✓ Cleared cache for {packageId} {version}");
        }
        else
        {
            AddLog($"ℹ No cache found for {packageId} {version}");
        }
    }

    private static (string packageId, string version) ExtractPackageInfo(string fileName)
    {
        if (!fileName.EndsWith(".nupkg", StringComparison.OrdinalIgnoreCase))
        {
            return (string.Empty, string.Empty);
        }

        var nameWithoutExt = fileName.Substring(0, fileName.Length - 6); // Remove .nupkg

        const string pattern = @"^(?<id>.+?)\.(?<version>\d+(\.\d+)*(\-[^\+]+)?(\+[^\s]+)?)$";

        var match = Regex.Match(nameWithoutExt, pattern);
        if (match.Success)
        {
            return (match.Groups["id"].Value, match.Groups["version"].Value);
        }

        return (string.Empty, string.Empty);
    }

    private static string ComputeCopyFolder(string watchFolder)
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

    private async Task<string> GetGlobalPackagesFolderAsync()
    {
        try
        {
            var startInfo = new ProcessStartInfo
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

            var match = Regex.Match(output, @"global-packages:\s*(.+)");
            if (match.Success)
            {
                return match.Groups[1].Value.Trim();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error getting global packages folder");
        }

        return string.Empty;
    }

    private void AddLog(string line)
    {
        lock (_stateLock)
        {
            // Cap log size to keep the UI list bounded.
            if (_activityLog.Count >= 200)
            {
                _activityLog.RemoveAt(_activityLog.Count - 1);
            }
            _activityLog.Insert(0, line);
        }
    }

    private void RaiseStateChanged() => StateChanged?.Invoke(this, EventArgs.Empty);

    /// <summary>
    /// Waits for a snapshot of pending file-processing tasks to complete. Bounded because
    /// the caller (<see cref="Stop"/>) cancels the token those tasks observe, so they
    /// exit at their next await rather than running to full completion.
    /// </summary>
    private static void DrainPending(List<Task> pending)
    {
        if (pending.Count == 0) return;
        try
        {
            Task.WaitAll(pending.ToArray(), TimeSpan.FromSeconds(5));
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "Timed out or failed draining NuGet watcher tasks on stop");
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;

        CancellationTokenSource cts;
        List<Task> pending;

        lock (_stateLock)
        {
            if (_watcher != null)
            {
                _watcher.EnableRaisingEvents = false;
                _watcher.Created -= FileCreated;
                _watcher.Dispose();
                _watcher = null;
            }
            pending = _pendingTasks.ToList();
            cts = _cts;
        }

        cts.Cancel();
        DrainPending(pending);
        cts.Dispose();
    }
}
