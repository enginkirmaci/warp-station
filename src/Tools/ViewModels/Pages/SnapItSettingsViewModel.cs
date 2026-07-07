using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Configuration;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

public partial class SnapItSettingsViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly ISnapItService _snapItService;
    private bool _isInitializing;

    [ObservableProperty]
    private bool _autoStart;

    [ObservableProperty]
    private string _statusText = "Checking...";

    [ObservableProperty]
    private bool _isRunning;

    [ObservableProperty]
    private string _toggleButtonText = "Start";

    public IRelayCommand ToggleSnapItCommand { get; }

    public SnapItSettingsViewModel(ISettingsService settingsService, ISnapItService snapItService)
    {
        _settingsService = settingsService;
        _snapItService = snapItService;

        ToggleSnapItCommand = new RelayCommand(OnToggleSnapIt);
        _snapItService.RunningChanged += OnSnapItRunningChanged;
    }

    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override Task OnNavigatedFromAsync()
    {
        // Detach from the singleton so this Transient VM (rebuilt per navigation) is not
        // kept alive by the service and does not receive further state changes.
        _snapItService.RunningChanged -= OnSnapItRunningChanged;
        return Task.CompletedTask;
    }

    public override async Task OnInitializeAsync()
    {
        _isInitializing = true;

        var appSettings = await _settingsService.GetSettingsAsync();
        AutoStart = appSettings.SnapIt?.AutoStart == true;

        UpdateStatus(_snapItService.IsRunning);

        _isInitializing = false;
    }

    private async void OnToggleSnapIt()
    {
        if (_snapItService.IsRunning)
        {
            _snapItService.Stop();
        }
        else
        {
            await _snapItService.StartAsync();
        }
    }

    private void OnSnapItRunningChanged(object? sender, bool isRunning)
    {
        UpdateStatus(isRunning);
    }

    private void UpdateStatus(bool isRunning)
    {
        IsRunning = isRunning;
        StatusText = isRunning ? "Running" : "Stopped";
        ToggleButtonText = isRunning ? "Stop" : "Start";
    }

    partial void OnAutoStartChanged(bool value)
    {
        if (_isInitializing) return;
        _ = SaveAutoStartAsync(value);
    }

    private async Task SaveAutoStartAsync(bool autoStart)
    {
        var appSettings = await _settingsService.GetSettingsAsync();
        appSettings.SnapIt ??= new SnapItSettings();
        appSettings.SnapIt.AutoStart = autoStart;
        await _settingsService.SaveSettingsAsync(appSettings);
    }
}
