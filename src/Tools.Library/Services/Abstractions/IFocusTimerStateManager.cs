using Tools.Library.Configuration;
using Tools.Library.Entities;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Manages the persistence and daily reset of the Focus Timer state.
/// </summary>
public interface IFocusTimerStateManager
{
    /// <summary>
    /// Loads or resets the state based on persistence and current date.
    /// </summary>
    Task<FocusTimerState> LoadStateAsync(FocusTimerSettings settings);

    /// <summary>
    /// Persists the current state to settings.
    /// </summary>
    Task PersistStateAsync(FocusTimerState state);
}
