using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Components.Tools;

/// <summary>
/// Code Execute tool (run C# snippets via Roslyn scripting), hosted in the main
/// window's floating tool drawer.
/// </summary>
public partial class CodeExecuteComponent : UserControl
{
    public CodeExecuteViewModel ViewModel { get; }

    public CodeExecuteComponent()
    {
        InitializeComponent();
    }

    public CodeExecuteComponent(CodeExecuteViewModel viewModel)
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
