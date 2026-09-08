using Avalonia;
using Tools.Library.Services.Abstractions;

namespace Tools.Services;

/// <summary>
/// Resolves the main window's <see cref="Avalonia.Controls.TopLevel"/> at call time from
/// the application's desktop lifetime. Deliberately not constructor-injected with the
/// window: <c>desktop.MainWindow</c> only exists after the DI container is built, so a
/// call-time lookup is the only cycle-free way for services to reach it.
/// </summary>
public class MainWindowProvider : IMainWindowProvider
{
    public Avalonia.Controls.TopLevel? TopLevel =>
        Application.Current?.ApplicationLifetime
            is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime { MainWindow: { } mainWindow }
                ? Avalonia.Controls.TopLevel.GetTopLevel(mainWindow)
                : null;
}
