using System.Text;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Clipboard Password page.
/// Manages password storage with Base64 encoding for clipboard hotkey functionality.
/// </summary>
public partial class ClipboardPasswordPageViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly INotificationService _notificationService;

    [ObservableProperty]
    private string _password = string.Empty;

    [ObservableProperty]
    private string _statusMessage = string.Empty;

    [ObservableProperty]
    private bool _hasStoredPassword;

    public ClipboardPasswordPageViewModel(ISettingsService settingsService, INotificationService notificationService)
    {
        _settingsService = settingsService;
        _notificationService = notificationService;
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => LoadPasswordStatusAsync();

    private async Task LoadPasswordStatusAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        HasStoredPassword = !string.IsNullOrEmpty(settings.ClipboardPassword?.EncryptedPassword);
        
        if (HasStoredPassword)
        {
            StatusMessage = "Password is stored and ready to use with Ctrl+Shift+V";
        }
        else
        {
            StatusMessage = "No password stored yet";
        }
    }

    [RelayCommand]
    private async Task SavePasswordAsync()
    {
        try
        {
            if (string.IsNullOrWhiteSpace(Password))
            {
                StatusMessage = "Please enter a password";
                return;
            }
            // Encode password to Base64
            var bytes = Encoding.UTF8.GetBytes(Password);
            var base64Password = Convert.ToBase64String(bytes);
            // Save to settings
            var settings = await _settingsService.GetSettingsAsync();
            if (settings.ClipboardPassword == null)
            {
                settings.ClipboardPassword = new();
            }
            settings.ClipboardPassword.EncryptedPassword = base64Password;
            await _settingsService.SaveSettingsAsync(settings);
            // Clear password field
            Password = string.Empty;
            HasStoredPassword = true;
            StatusMessage = "Password saved successfully! Use Ctrl+Shift+V to paste it.";
            _notificationService.Show("Password saved", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            StatusMessage = $"Error saving password: {ex.Message}";
            _notificationService.Show("Failed to save password", NotificationKind.Error);
        }
    }

    [RelayCommand]
    private async Task ClearPasswordAsync()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            if (settings.ClipboardPassword != null)
            {
                settings.ClipboardPassword.EncryptedPassword = null;
                await _settingsService.SaveSettingsAsync(settings);
            }
            Password = string.Empty;
            HasStoredPassword = false;
            StatusMessage = "Password cleared";
            _notificationService.Show("Password cleared", NotificationKind.Success);
        }
        catch (Exception ex)
        {
            StatusMessage = $"Error clearing password: {ex.Message}";
            _notificationService.Show("Failed to clear password", NotificationKind.Error);
        }
    }
}
