using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Long-running NuGet local watch service. Decouples the file system watcher
/// lifecycle from the Nuget Local page so that watching continues even when
/// the user navigates away from the page.
/// </summary>
public interface INugetLocalService
{
    /// <summary>Gets the folder currently being watched for new .nupkg files.</summary>
    string WatchFolder { get; }

    /// <summary>Gets the computed destination folder (&lt;WatchFolder&gt;\nugets) where packages are copied.</summary>
    string ComputedCopyFolder { get; }

    /// <summary>Gets the NuGet global packages cache folder.</summary>
    string GlobalPackagesFolder { get; }

    /// <summary>Gets a value indicating whether watching is currently active.</summary>
    bool IsWatching { get; }

    /// <summary>Gets the number of packages processed in the current interval window.</summary>
    int Count { get; }

    /// <summary>Gets the most recent activity log lines.</summary>
    IReadOnlyList<string> ActivityLog { get; }

    /// <summary>Raised whenever any service state changes (watch status, count, log, paths).</summary>
    event EventHandler? StateChanged;

    /// <summary>Gets the persisted NuGet local settings.</summary>
    Task<NugetLocalSettings> GetSettingsAsync();

    /// <summary>Persists the watch folder to settings and updates the computed copy folder.</summary>
    Task SetWatchFolderAsync(string? path);

    /// <summary>Starts watching the current <see cref="WatchFolder"/> for new packages.</summary>
    /// <returns>True if watching started (or was already running); false if the folder is invalid.</returns>
    Task<bool> StartAsync();

    /// <summary>Stops watching and releases the current file system watcher.</summary>
    void Stop();

    /// <summary>Registers the computed copy folder as a local NuGet source.</summary>
    Task RegisterSourceAsync();
}
