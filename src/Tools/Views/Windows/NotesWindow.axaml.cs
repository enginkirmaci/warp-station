using System.ComponentModel;
using Avalonia.Controls;
using SukiUI.Controls;
using Tools.ViewModels.Pages;
using Tools.Views.Pages;

namespace Tools.Views.Windows;

/// <summary>
/// Floating window hosting the Notes page: the repositories table stays visible in
/// the main window while notes are open. The page instance and its singleton view
/// model are shared with MainWindow, so unsaved-note buffers and tree state are the
/// same however the window is reached. Every user close — the caption button or the
/// page's back link — only hides the window (position and visual tree survive);
/// <see cref="ForceClose"/> is shutdown-only, reached from the main window's close.
/// </summary>
public partial class NotesWindow : SukiWindow
{
    private readonly NotesPageViewModel _viewModel;
    private bool _forceClose;

    public NotesWindow(NotesPage page, NotesPageViewModel viewModel)
    {
        _viewModel = viewModel;
        InitializeComponent();
        Content = page;
        viewModel.PropertyChanged += OnViewModelPropertyChanged;
        Closed += (_, _) =>
        {
            viewModel.PropertyChanged -= OnViewModelPropertyChanged;
            // A closed window keeps its content attached to its visual tree; detach
            // explicitly so the page can be hosted again if a window is ever
            // re-created for it.
            Content = null;
        };
        UpdateScope();
    }

    /// <summary>Real close — a plain Close() only hides the window.</summary>
    internal void ForceClose()
    {
        _forceClose = true;
        Close();
    }

    protected override void OnClosing(WindowClosingEventArgs e)
    {
        if (!_forceClose)
        {
            e.Cancel = true;
            Hide();
            return;
        }
        base.OnClosing(e);
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(NotesPageViewModel.RepoName) or nameof(NotesPageViewModel.HasRepo))
        {
            UpdateScope();
        }
    }

    /// <summary>The window chrome carries the scope: the title (taskbar, alt-tab) and
    /// the repo name beside the "Notes" brand while the view is repo-scoped.</summary>
    private void UpdateScope()
    {
        Title = _viewModel.HasRepo ? $"Notes — {_viewModel.RepoName}" : "Notes";
        if (this.FindControl<TextBlock>("ScopeText") is { } scope)
        {
            scope.Text = _viewModel.RepoName;
            scope.IsVisible = _viewModel.HasRepo;
        }
    }
}
