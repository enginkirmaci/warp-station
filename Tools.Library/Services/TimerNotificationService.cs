using Windows.Media.Core;
using Windows.Media.Playback;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerNotificationService.
/// </summary>
public class TimerNotificationService : ITimerNotificationService
{
    private readonly ISettingsService _settingsService;
    private readonly MediaPlayer _mediaPlayer;

    public TimerNotificationService(ISettingsService settingsService)
    {
        _settingsService = settingsService;
        _mediaPlayer = new MediaPlayer();
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
            // Ignore sound errors
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
            // Ignore sound errors
        }
    }

    private void PlaySound(string relativePath)
    {
        try
        {
            var fullPath = Path.Combine(AppContext.BaseDirectory, relativePath);
            if (File.Exists(fullPath))
            {
                _mediaPlayer.Source = MediaSource.CreateFromUri(new Uri(fullPath));
                _mediaPlayer.Play();
            }
        }
        catch
        {
            // Ignore sound errors
        }
    }

    public void RequestWindowVisibility(bool show)
    {
        VisibilityRequested?.Invoke(this, show);
    }
}
