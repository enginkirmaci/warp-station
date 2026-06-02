using System.Media;
using System.Runtime.Versioning;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerNotificationService.
/// Uses platform-specific audio playback.
/// </summary>
public class TimerNotificationService : ITimerNotificationService
{
    private readonly ISettingsService _settingsService;

    public TimerNotificationService(ISettingsService settingsService)
    {
        _settingsService = settingsService;
    }

    public event EventHandler<bool>? VisibilityRequested;

    public async void PlayBreakSound()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var soundPath = settings.FocusTimer?.BreakSoundPath ?? "Assets/break.mp3";
            PlaySound(soundPath);
        }
        catch
        {
        }
    }

    public async void PlayFocusSound()
    {
        try
        {
            var settings = await _settingsService.GetSettingsAsync();
            var soundPath = settings.FocusTimer?.FocusSoundPath ?? "Assets/focus.mp3";
            PlaySound(soundPath);
        }
        catch
        {
        }
    }

    [SupportedOSPlatform("windows")]
    private void PlaySound(string relativePath)
    {
        try
        {
            var fullPath = Path.Combine(AppContext.BaseDirectory, relativePath);
            if (File.Exists(fullPath))
            {
                var player = new SoundPlayer(fullPath);
                player.Play();
            }
        }
        catch
        {
        }
    }

    public void RequestWindowVisibility(bool show)
    {
        VisibilityRequested?.Invoke(this, show);
    }
}
