using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Singleton orchestrator that owns discovered workspace/platform data, coordinating
/// <see cref="IWorkspaceScanner"/> and <see cref="IWorkspaceCacheStore"/>. This replaces
/// the static collections previously held in the Workspaces ViewModel (a workaround for
/// Transient page lifetimes) with proper singleton-owned state.
/// </summary>
public class WorkspaceService : IWorkspaceService
{
    private readonly IWorkspaceScanner _scanner;
    private readonly IWorkspaceCacheStore _cacheStore;

    private List<WorkspaceItem> _workspaces = new();
    private List<WorkspaceItem> _platforms = new();
    private bool _busy;
    private bool _cacheLoaded;

    public WorkspaceService(IWorkspaceScanner scanner, IWorkspaceCacheStore cacheStore)
    {
        _scanner = scanner;
        _cacheStore = cacheStore;
    }

    /// <inheritdoc/>
    public IReadOnlyList<WorkspaceItem> Workspaces => _workspaces;

    /// <inheritdoc/>
    public IReadOnlyList<WorkspaceItem> Platforms => _platforms;

    /// <inheritdoc/>
    public bool IsBusy => _busy;

    /// <inheritdoc/>
    public event EventHandler? Changed;

    /// <inheritdoc/>
    public async Task EnsureLoadedAsync(WorkspacesSettings settings)
    {
        // Load cache once if we have no data yet, so the UI renders instantly.
        if (!_cacheLoaded && _workspaces.Count == 0 && _platforms.Count == 0)
        {
            var cache = await _cacheStore.LoadAsync();
            if (cache != null)
            {
                _workspaces = cache.Workspaces ?? new List<WorkspaceItem>();
                _platforms = cache.Platforms ?? new List<WorkspaceItem>();
                _cacheLoaded = true;
                RaiseChanged();
            }
        }

        // Always scan in background to keep data fresh, even if we loaded from cache.
        if (settings.WorkspaceScanFolders?.Any() == true)
        {
            _ = ScanAsync(settings);
        }
    }

    /// <inheritdoc/>
    public async Task RefreshAsync(WorkspacesSettings settings)
    {
        _workspaces = new List<WorkspaceItem>();
        _platforms = new List<WorkspaceItem>();
        _cacheLoaded = false;
        RaiseChanged();
        await EnsureLoadedAsync(settings);
    }

    private async Task ScanAsync(WorkspacesSettings settings)
    {
        if (_busy) return;
        _busy = true;
        RaiseChanged();

        try
        {
            var result = await _scanner.ScanAsync(settings);
            _workspaces = result.Workspaces;
            _platforms = result.Platforms;

            await _cacheStore.SaveAsync(new WorkspaceCache
            {
                Workspaces = _workspaces.ToList(),
                Platforms = _platforms.ToList()
            });

            RaiseChanged();
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error scanning workspaces");
        }
        finally
        {
            _busy = false;
            RaiseChanged();
        }
    }

    private void RaiseChanged() => Changed?.Invoke(this, EventArgs.Empty);
}
