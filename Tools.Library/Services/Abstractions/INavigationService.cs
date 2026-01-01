using Microsoft.UI.Xaml.Controls;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides navigation services for the application.
/// </summary>
public interface INavigationService
{
    /// <summary>
    /// Gets or sets the frame used for navigation.
    /// </summary>
    Frame? Frame { get; set; }

    /// <summary>
    /// Gets a value indicating whether the navigation service can navigate back.
    /// </summary>
    bool CanGoBack { get; }

    /// <summary>
    /// Navigates to the specified page type.
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
    /// Sets the frame to be used for navigation.
    /// </summary>
    /// <param name="frame">The frame to set.</param>
    void SetFrame(Frame frame);

    /// <summary>
    /// Event raised after navigation to notify subscribers of the new page type.
    /// </summary>
    event Action<Type?>? Navigated;

    /// <summary>
    /// Event raised when back stack availability changes.
    /// </summary>
    event Action? BackStackChanged;
}
