using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Converters;
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Providers;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

public partial class DashboardViewModel : PageViewModelBase
{
    private readonly INavigationService _navigationService;
    private readonly ISnapItService _snapItService;

    [ObservableProperty]
    private bool _snapItRunning;

    [ObservableProperty]
    private string _snapItStatusText = "Checking...";

    [ObservableProperty]
    private string _snapItToggleText = "Start";

    public IRelayCommand<string> CardClickCommand { get; }
    public IRelayCommand ToggleSnapItCommand { get; }

    public IReadOnlyCollection<NavigationItem> DashboardCards { get; }

    public DashboardViewModel(INavigationService navigationService, ISnapItService snapItService)
    {
        _navigationService = navigationService;
        _snapItService = snapItService;
        CardClickCommand = new RelayCommand<string>(OnCardClick);
        ToggleSnapItCommand = new RelayCommand(OnToggleSnapIt);
        DashboardCards = NavigationProvider.GetDashboardItems(CardClickCommand);
        _snapItService.RunningChanged += OnSnapItRunningChanged;
        UpdateSnapItStatus(_snapItService.IsRunning);
    }

    private void OnCardClick(string? parameter)
    {
        if (string.IsNullOrWhiteSpace(parameter))
        {
            return;
        }
        Type? pageType = NameToPageTypeConverter.Convert(parameter);
        if (pageType == null)
        {
            return;
        }
        _navigationService.Navigate(pageType);
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
        UpdateSnapItStatus(isRunning);
    }

    private void UpdateSnapItStatus(bool isRunning)
    {
        SnapItRunning = isRunning;
        SnapItStatusText = isRunning ? "Running" : "Stopped";
        SnapItToggleText = isRunning ? "Stop" : "Start";
    }
}