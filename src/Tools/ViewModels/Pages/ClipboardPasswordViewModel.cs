using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Configuration;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Clipboard Password page. Manages password storage with Base64
/// encoding for clipboard hotkey functionality (the encode + persist live in
/// <see cref="IClipboardPasswordService.SetPasswordAsync"/>; clearing persists through
/// <see cref="ISettingsService.UpdateAsync{TSection}"/> — both single-lock updates. This
/// VM only orchestrates the UI state around them).
/// </summary>
public partial class ClipboardPasswordViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly INotificationService _notificationService;
    private readonly IClipboardPasswordService _clipboardPasswordService;

    [ObservableProperty]
    private string _password = string.Empty;

    [ObservableProperty]
    private string _statusMessage = string.Empty;

    [ObservableProperty]
    private bool _hasStoredPassword;

    public ClipboardPasswordViewModel(ISettingsService settingsService, INotificationService notificationService,
        IClipboardPasswordService clipboardPasswordService)
    {
        _settingsService = settingsService;
        _notificationService = notificationService;
        _clipboardPasswordService = clipboardPasswordService;
    }

    /// <inheritdoc/>
    public override Task OnNavigatedToAsync(object? parameter = null) => OnInitializeAsync();

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
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
            // The service encodes (Base64 over UTF-8) and persists the password
            await _clipboardPasswordService.SetPasswordAsync(Password);
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
            // Single-lock update: clears EncryptedPassword on the persisted graph without
            // a hand-rolled get → mutate → save round trip (the section is guaranteed
            // non-null by the settings service).
            await _settingsService.UpdateAsync<ClipboardPasswordSettings>(
                cp => cp.EncryptedPassword = null);
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
