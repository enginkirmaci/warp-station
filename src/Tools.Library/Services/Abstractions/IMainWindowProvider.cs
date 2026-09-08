using Avalonia.Controls;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Gives services access to the main window's UI surface (storage pickers, clipboard)
/// without depending on the concrete MainWindow view. The window is resolved at call
/// time from the application's lifetime, so consumers can be built before the window
/// exists and the DI graph never cycles back into a view.
/// </summary>
public interface IMainWindowProvider
{
    /// <summary>
    /// Gets the main window's <see cref="TopLevel"/> once the window is attached to the
    /// presentation source, or <c>null</c> before that (startup, headless operation).
    /// </summary>
    TopLevel? TopLevel { get; }
}
