using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.Library.Configuration;

namespace Tools.Views.Windows;

public partial class WorkspaceSettingsDialog : Window
{
    public WorkspacesSettings Settings { get; set; } = new();

    public WorkspaceSettingsDialog()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
