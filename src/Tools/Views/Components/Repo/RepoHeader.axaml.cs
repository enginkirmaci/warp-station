using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Components;

namespace Tools.Views.Components;

/// <summary>
/// Repo detail header at the top of the Repositories page (back link, repo name + star,
/// branch / GitHub URL line, tab row, Open in GitHub + kebab). Its DataContext is the
/// singleton <see cref="BottomBarViewModel"/> attached by ReposPage — the same VM as the
/// bottom bar, so the tabs here drive the bar's panel.
/// </summary>
public partial class RepoHeader : UserControl
{
    public RepoHeader()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
