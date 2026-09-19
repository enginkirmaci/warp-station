using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Input;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

/// <summary>Code-behind of the Notes page: navigation back, the editor's Ctrl+S, the
/// search box's Enter/Escape, search-result opening and the new note/folder flyouts.
/// Everything stateful lives in <see cref="NotesPageViewModel"/>.</summary>
public partial class NotesPage : UserControl
{
    public event EventHandler? BackRequested;

    /// <summary>XAML compiler requirement (AVLN3000); DI resolves the ViewModel ctor below.</summary>
    public NotesPage()
    {
        InitializeComponent();
    }

    public NotesPage(NotesPageViewModel viewModel)
    {
        DataContext = viewModel;
        InitializeComponent();
    }

    private NotesPageViewModel? ViewModel => DataContext as NotesPageViewModel;

    private void OnBackClick(object? sender, RoutedEventArgs e)
        => BackRequested?.Invoke(this, EventArgs.Empty);

    private void OnNoteEditorKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.S && e.KeyModifiers.HasFlag(KeyModifiers.Control) && ViewModel is { } vm)
        {
            vm.SaveCommand.Execute(null);
            e.Handled = true;
        }
    }

    private void OnSearchBoxKeyDown(object? sender, KeyEventArgs e)
    {
        if (ViewModel is not { } vm)
        {
            return;
        }

        if (e.Key == Key.Enter)
        {
            vm.RunSearchNowCommand.Execute(null);
            e.Handled = true;
        }
        else if (e.Key == Key.Escape)
        {
            vm.ClearSearchCommand.Execute(null);
            e.Handled = true;
        }
    }

    private void OnSearchResultSelected(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ListBox { SelectedItem: NotesSearchHit hit } list && ViewModel is { } vm)
        {
            _ = vm.OpenSearchResultAsync(hit);
            list.SelectedItem = null;
        }
    }

    private void OnNewNoteFlyoutOpened(object? sender, EventArgs e)
        => FocusFlyoutTextBox(sender);

    private void OnNewFolderFlyoutOpened(object? sender, EventArgs e)
        => FocusFlyoutTextBox(sender);

    private static void FocusFlyoutTextBox(object? sender)
    {
        if (sender is Flyout { Content: StackPanel panel })
        {
            panel.Children.OfType<TextBox>().FirstOrDefault()?.Focus();
        }
    }

    private void OnNewNoteNameKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter && ViewModel is { } vm)
        {
            vm.CreateNoteCommand.Execute(null);
            HideFlyout("NewNoteButton");
            e.Handled = true;
        }
    }

    private void OnNewNoteCreateClick(object? sender, RoutedEventArgs e)
    {
        if (ViewModel is { } vm)
        {
            vm.CreateNoteCommand.Execute(null);
            HideFlyout("NewNoteButton");
        }
    }

    private void OnNewFolderNameKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter && ViewModel is { } vm)
        {
            vm.CreateFolderCommand.Execute(null);
            HideFlyout("NewFolderButton");
            e.Handled = true;
        }
    }

    private void OnNewFolderCreateClick(object? sender, RoutedEventArgs e)
    {
        if (ViewModel is { } vm)
        {
            vm.CreateFolderCommand.Execute(null);
            HideFlyout("NewFolderButton");
        }
    }

    private void HideFlyout(string hostButtonName)
    {
        if (this.FindControl<Button>(hostButtonName)?.Flyout is Flyout flyout && flyout.IsOpen)
        {
            flyout.Hide();
        }
    }
}
