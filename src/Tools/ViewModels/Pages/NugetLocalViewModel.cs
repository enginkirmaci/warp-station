using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Serilog;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Nuget Local page. Acts as a thin binding adapter over the
/// long-running <see cref="INugetLocalService"/>; the watch itself survives
/// navigation away from this page because it lives on the singleton service.
/// </summary>
public partial class NugetLocalViewModel : PageViewModelBase
{
    private readonly INugetLocalService _nugetService;
    private readonly IDialogService _dialogService;

    /// <summary>
    /// Exposes the singleton service for direct XAML binding (paths, status, log).
    /// </summary>
    public INugetLocalService Service => _nugetService;

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

    [ObservableProperty]
    private ObservableCollection<string> _fileList = new();

    /// <summary>Gets the command to start/stop watching for changes.</summary>
    public IRelayCommand<object> WatchChangesCommand { get; }

    /// <summary>Gets the command to select folders.</summary>
    public IAsyncRelayCommand<string> SelectFolderCommand { get; }

    /// <summary>Gets the command to register the watch folder as a NuGet source.</summary>
    public IAsyncRelayCommand RegisterSourceCommand { get; }

    public NugetLocalViewModel(INugetLocalService nugetService, IDialogService dialogService)
    {
        _nugetService = nugetService;
        _dialogService = dialogService;

        WatchChangesCommand = new RelayCommand<object>(OnWatchChanges);
        SelectFolderCommand = new AsyncRelayCommand<string>(OnSelectFolderAsync);
        RegisterSourceCommand = new AsyncRelayCommand(OnRegisterSourceAsync);

        _nugetService.StateChanged += OnServiceStateChanged;
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override Task OnNavigatedFromAsync()
    {
        // Detach from the singleton so this Transient VM (rebuilt per navigation) is not
        // kept alive by the service and does not receive further state changes.
        _nugetService.StateChanged -= OnServiceStateChanged;
        return Task.CompletedTask;
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        // Seed local properties from the service snapshot.
        SyncFromService();

        // Global packages folder may not have been resolved yet; refresh once.
        GlobalPackagesFolder = _nugetService.GlobalPackagesFolder;
        await Task.CompletedTask;
    }

    private void SyncFromService()
    {
        WatchFolder = _nugetService.WatchFolder;
        ComputedCopyFolder = _nugetService.ComputedCopyFolder;
        GlobalPackagesFolder = _nugetService.GlobalPackagesFolder;
        WatchStarted = _nugetService.IsWatching;
        Count = _nugetService.Count;

        // Mirror the service log into the observable collection bound to the UI.
        FileList = new ObservableCollection<string>(_nugetService.ActivityLog);
    }

    private void OnServiceStateChanged(object? sender, EventArgs e)
    {
        Avalonia.Threading.Dispatcher.UIThread.Post(() =>
        {
            WatchStarted = _nugetService.IsWatching;
            Count = _nugetService.Count;
            ComputedCopyFolder = _nugetService.ComputedCopyFolder;
            GlobalPackagesFolder = _nugetService.GlobalPackagesFolder;

            // Update the log collection in place to keep it bound/scrolled.
            if (!ReferenceEquals(FileList, _nugetService.ActivityLog))
            {
                FileList = new ObservableCollection<string>(_nugetService.ActivityLog);
            }
        });
    }

    partial void OnWatchFolderChanged(string value)
    {
        // Persist + update computed path on the service. Avoid echoing back into
        // an infinite loop by only pushing when it actually differs.
        if (!string.Equals(value, _nugetService.WatchFolder, StringComparison.Ordinal))
        {
            _ = _nugetService.SetWatchFolderAsync(value);
            ComputedCopyFolder = _nugetService.ComputedCopyFolder;
        }
    }

    private async Task OnSelectFolderAsync(string? operation)
    {
        try
        {
            var path = await _dialogService.PickFolderAsync("Select Watch Folder");
            if (path != null && operation == "Watch")
            {
                await _nugetService.SetWatchFolderAsync(path);
                SyncFromService();
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Avalonia FolderPicker failed");
        }
    }

    private void OnWatchChanges(object? started)
    {
        if (started is bool isStarted && isStarted)
        {
            _ = _nugetService.StartAsync();
        }
        else
        {
            _nugetService.Stop();
        }
    }

    private Task OnRegisterSourceAsync()
    {
        return _nugetService.RegisterSourceAsync();
    }
}
