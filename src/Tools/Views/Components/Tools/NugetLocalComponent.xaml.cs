using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Components;

/// <summary>
/// NuGet Package Manager tool, hosted in the main window's floating tool drawer.
/// A thin view over <see cref="NugetLocalViewModel"/>; the watch itself lives on the
/// singleton <see cref="Tools.Library.Services.Abstractions.INugetLocalService"/>.
/// </summary>
public partial class NugetLocalComponent : UserControl
{
    public NugetLocalViewModel ViewModel { get; }

    public NugetLocalComponent()
    {
        InitializeComponent();
    }

    public NugetLocalComponent(NugetLocalViewModel viewModel)
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
