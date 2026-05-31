using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerNotificationService.
/// Uses platform-specific audio playback.
/// </summary>
public class TimerNotificationService : ITimerNotificationService
{
    private readonly ISettingsService _settingsService;

#if WINDOWS
    private readonly Windows.Media.Playback.MediaPlayer _mediaPlayer;

    public TimerNotificationService(ISettingsService settingsService)
    {
        _settingsService = settingsService;
        _mediaPlayer = new Windows.Media.Playback.MediaPlayer();
    }
#else
    public TimerNotificationService(ISettingsService settingsService)
    {
        _settingsService = settingsService;
    }
#endif

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
#if WINDOWS
        try
        {
            var fullPath = Path.Combine(AppContext.BaseDirectory, relativePath);
            if (File.Exists(fullPath))
            {
                _mediaPlayer.Source = Windows.Media.Core.MediaSource.CreateFromUri(new Uri(fullPath));
                _mediaPlayer.Play();
            }
        }
        catch
        {
            // Ignore sound errors
        }
#endif
    }

    public void RequestWindowVisibility(bool show)
    {
        VisibilityRequested?.Invoke(this, show);
    }
}
