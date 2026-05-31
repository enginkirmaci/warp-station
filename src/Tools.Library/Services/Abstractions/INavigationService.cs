using Avalonia.Controls;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides framework-agnostic navigation services for the application.
/// </summary>
public interface INavigationService
{
    /// <summary>
    /// Gets a value indicating whether the navigation service can navigate back.
    /// </summary>
    bool CanGoBack { get; }

    /// <summary>
    /// Navigates to the page of the specified type.
    /// </summary>
    /// <param name="pageType">The type of the page to navigate to.</param>
    /// <param name="parameter">Optional parameter to pass to the page.</param>
    /// <returns>True if navigation was successful; otherwise, false.</returns>
    bool Navigate(Type pageType, object? parameter = null);

    /// <summary>
    /// Navigates back to the previous page.
    /// </summary>
    /// <returns>True if navigation was successful; otherwise, false.</returns>
    bool GoBack();

    /// <summary>
    /// Sets the content control used to display pages.
    /// </summary>
    void SetContentControl(ContentControl contentControl);

    /// <summary>
    /// Event raised after navigation to notify subscribers of the new page type.
    /// </summary>
    event Action<Type?>? Navigated;

    /// <summary>
    /// Event raised when back stack availability changes.
    /// </summary>
    event Action? BackStackChanged;
}
