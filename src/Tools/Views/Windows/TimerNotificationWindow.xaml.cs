using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Windows;

namespace Tools.Views.Windows;

public partial class TimerNotificationWindow : Window
{
    public TimerNotificationWindowViewModel ViewModel { get; }

    public TimerNotificationWindow()
    {
        InitializeComponent();
    }

    public TimerNotificationWindow(TimerNotificationWindowViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
    }

    public void ShowWindow() { Show(); }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
