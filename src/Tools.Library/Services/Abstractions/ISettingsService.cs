using Tools.Library.Configuration;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides settings management for the application.
/// </summary>
public interface ISettingsService
{
    /// <summary>
    /// Loads settings from storage asynchronously.
    /// </summary>
    /// <returns>The application settings.</returns>
    Task<AppSettings> LoadSettingsAsync();

    /// <summary>
    /// Gets the current settings asynchronously.
    /// </summary>
    /// <returns>The application settings.</returns>
    Task<AppSettings> GetSettingsAsync();

    /// <summary>
    /// Saves settings to storage asynchronously.
    /// </summary>
    /// <param name="settings">The settings to save.</param>
    Task SaveSettingsAsync(AppSettings settings);

    /// <summary>
    /// Applies <paramref name="mutate"/> to a deep copy of the current settings and
    /// persists the result as one get → mutate → save transition. Read, mutation and
    /// adoption of the new state happen under a single lock, so concurrent updates
    /// stack instead of racing (unlike a hand-rolled <see cref="GetSettingsAsync"/> →
    /// mutate → <see cref="SaveSettingsAsync"/> round trip, where two overlapping
    /// single-field saves resurrect stale copies of each other's sections). Returns a
    /// deep copy of the newly persisted state.
    /// </summary>
    Task<AppSettings> UpdateAsync(Action<AppSettings> mutate);

    /// <summary>
    /// Section-scoped variant of <see cref="UpdateAsync(Action{AppSettings})"/>: resolves
    /// the nested section of type <typeparamref name="TSection"/> on a deep copy of the
    /// current settings, applies <paramref name="mutate"/> to it and persists the whole
    /// graph under the same single-lock discipline, e.g.
    /// <c>await settings.UpdateAsync&lt;ReposSettings&gt;(r =&gt; r.MaxScanDepth = 5);</c>
    /// </summary>
    Task<AppSettings> UpdateAsync<TSection>(Action<TSection> mutate) where TSection : class;
}