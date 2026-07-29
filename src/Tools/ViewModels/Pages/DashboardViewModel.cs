using CommunityToolkit.Mvvm.Input;
using Tools.Helpers;
using Tools.Library.Entities;
using Tools.Library.Mvvm;
using Tools.Library.Providers;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

public partial class DashboardViewModel : PageViewModelBase
{
    private readonly INavigationService _navigationService;

    public IReadOnlyCollection<NavigationItem> DashboardCards { get; }

    public DashboardViewModel(INavigationService navigationService, ISettingsService settingsService)
    {
        _navigationService = navigationService;
        // Read the hide flag synchronously: GetSettingsAsync is an in-memory cached
        // read (Task.FromResult), so this never blocks on async work.
        var hideClipboardPassword = settingsService
            .GetSettingsAsync()
            .GetAwaiter()
            .GetResult()
            .ClipboardPassword?.HideFromGui == true;
        DashboardCards = NavigationProvider.GetDashboardItems(CardClickCommand, hideClipboardPassword);
    }

    /// <summary>
    /// Navigates to the page keyed by the clicked card's <paramref name="parameter"/>.
    /// </summary>
    [RelayCommand]
    private void CardClick(string? parameter)
    {
        if (string.IsNullOrWhiteSpace(parameter))
        {
            return;
        }
        Type? pageType = PageNavigationMapper.Convert(parameter);
        if (pageType == null)
        {
            return;
        }
        _navigationService.Navigate(pageType);
    }
}
