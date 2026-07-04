using System.Media;
using System.Runtime.Versioning;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implementation of ITimerNotificationService.
/// Uses platform-specific audio playback.
/// </summary>
public class TimerNotificationService : ITimerNotificationService, IDisposable
{
    private readonly ISettingsService _settingsService;
    // SoundPlayer is IDisposable and holds a wave-out resource. Cache one
    // instance per sound path and reuse it instead of allocating (and leaking)
    // a new player on every notification.
    private readonly Dictionary<string, SoundPlayer> _players = new();
    private bool _disposed;

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
        if (_disposed)
        {
            return;
        }

        try
        {
            var fullPath = Path.Combine(AppContext.BaseDirectory, relativePath);
            if (!File.Exists(fullPath))
            {
                return;
            }

            // Reuse a cached player for this path; recreate only if the path changed.
            if (!_players.TryGetValue(fullPath, out var player))
            {
                player = new SoundPlayer(fullPath);
                _players[fullPath] = player;
            }

            player.Play();
        }
        catch
        {
        }
    }

    public void RequestWindowVisibility(bool show)
    {
        VisibilityRequested?.Invoke(this, show);
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        foreach (var player in _players.Values)
        {
            player.Dispose();
        }

        _players.Clear();
        _disposed = true;
    }
}
