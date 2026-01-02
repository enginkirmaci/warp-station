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
}