using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Components;

/// <summary>
/// Clipboard Password tool (store a password, paste via Ctrl+Shift+V), hosted in the
/// main window's floating tool drawer. The tool itself can be hidden from the GUI via
/// the EnableClipboardPassword setting; the drawer entry is filtered in the title-bar dropdown.
/// </summary>
public partial class ClipboardPasswordComponent : UserControl
{
    public ClipboardPasswordViewModel ViewModel { get; }

    public ClipboardPasswordComponent()
    {
        InitializeComponent();
    }

    public ClipboardPasswordComponent(ClipboardPasswordViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
