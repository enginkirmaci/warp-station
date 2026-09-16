using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using OpenCodeAgent.Views;

namespace OpenCodeAgent;

public partial class App : Application
{
    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            desktop.MainWindow = new MainWindow();
            if (desktop.MainWindow is MainWindow mainWindow)
                _ = mainWindow.InitializeAsync();
        }
        base.OnFrameworkInitializationCompleted();
    }
}
