using Tools.Library.Configuration;
using Tools.Library.Entities;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// Implements state management and persistence for the Focus Timer.
/// </summary>
public class FocusTimerStateManager : IFocusTimerStateManager
{
    private readonly ISettingsService _settingsService;

    public FocusTimerStateManager(ISettingsService settingsService)
    {
        _settingsService = settingsService;
    }

    public async Task<FocusTimerState> LoadStateAsync(FocusTimerSettings settings)
    {
        var today = DateTime.Today.ToString("yyyy-MM-dd");
        var persistedState = settings.PersistedState;

        if (persistedState == null || persistedState.LastResetDate != today)
        {
            // Reset for new day
            return new FocusTimerState
            {
                CurrentBreakPoolMinutes = settings.TotalDailyBreakMinutes,
                RemainingBreakCount = settings.DesiredBreakCount,
                TotalDailyBreakMinutes = settings.TotalDailyBreakMinutes,
                BreakTimeTakenMinutes = 0,
                LastBreakEndTime = null
            };
        }
        else
        {
            // Restore persisted state
            return new FocusTimerState
            {
                CurrentBreakPoolMinutes = persistedState.CurrentBreakPoolMinutes,
                RemainingBreakCount = persistedState.RemainingBreakCount,
                TotalDailyBreakMinutes = settings.TotalDailyBreakMinutes,
                BreakTimeTakenMinutes = persistedState.BreakTimeTakenMinutes,
                LastBreakEndTime = DateTime.Now // Approximate for continuation
            };
        }
    }

    public async Task PersistStateAsync(FocusTimerState state)
    {
        var appSettings = await _settingsService.GetSettingsAsync();
        if (appSettings.FocusTimer == null)
        {
            appSettings.FocusTimer = new FocusTimerSettings();
        }

        appSettings.FocusTimer.PersistedState = new FocusTimerPersistedState
        {
            LastResetDate = DateTime.Today.ToString("yyyy-MM-dd"),
            CurrentBreakPoolMinutes = state.CurrentBreakPoolMinutes,
            RemainingBreakCount = state.RemainingBreakCount,
            BreakTimeTakenMinutes = state.BreakTimeTakenMinutes
        };

        await _settingsService.SaveSettingsAsync(appSettings);
    }
}
