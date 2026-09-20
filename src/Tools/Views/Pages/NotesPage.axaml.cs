using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Input;
using MarkdownViewerKit;
using Tools.Helpers;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

/// <summary>Code-behind of the Notes page: navigation back, the editor's Ctrl+S and
/// markdown keys, the formatting toolbar, selection-driven status readout, the search
/// box's Enter/Escape, search-result opening and the new note/folder flyouts. Text
/// transforms live in <see cref="MarkdownEditing"/>, state in <see cref="NotesPageViewModel"/>.
/// </summary>
public partial class NotesPage : UserControl
{
    public event EventHandler? BackRequested;

    /// <summary>XAML compiler requirement (AVLN3000); DI resolves the ViewModel ctor below.</summary>
    public NotesPage()
    {
        InitializeComponent();
        AttachEditorKeyHandling();
    }

    public NotesPage(NotesPageViewModel viewModel)
    {
        DataContext = viewModel;
        InitializeComponent();
        AttachEditorKeyHandling();
    }

    private NotesPageViewModel? ViewModel => DataContext as NotesPageViewModel;

    /// <summary>The tunnel handler must outrun the AcceptsReturn TextBox's own Enter
    /// handling (a bubbling KeyDown never sees plain Enter) and Tab's focus navigation.
    /// Selection is tracked per keystroke while an editor holds focus: clicking a
    /// toolbar button moves focus away and the TextBox clears its selection.</summary>
    private void AttachEditorKeyHandling()
    {
        AddHandler(KeyDownEvent, OnEditorKeyDown, RoutingStrategies.Tunnel);
        foreach (var name in new[] { "NoteEditor", "NoteEditorSplit" })
        {
            if (this.FindControl<TextBox>(name) is { } editor)
            {
                editor.GetObservable(TextBox.SelectionStartProperty).Subscribe(new CaretObserver<int>(_ => TrackEditorSelection(editor)));
                editor.GetObservable(TextBox.SelectionEndProperty).Subscribe(new CaretObserver<int>(_ => TrackEditorSelection(editor)));
                editor.GetObservable(TextBox.CaretIndexProperty).Subscribe(new CaretObserver<int>(_ => TrackEditorSelection(editor)));
            }
        }
    }

    private TextBox? _focusedEditor;
    private int _selStart;
    private int _selLength;

    private void TrackEditorSelection(TextBox editor)
    {
        if (!editor.IsFocused)
        {
            return;
        }

        _focusedEditor = editor;
        _selStart = editor.SelectionStart;
        _selLength = Math.Max(0, editor.SelectionEnd - editor.SelectionStart);
        (DataContext as NotesPageViewModel)?.UpdateCursorPosition(editor.CaretIndex);
    }

    private void OnEditorKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Source is not TextBox { Name: "NoteEditor" or "NoteEditorSplit" } editor
            || ViewModel is not { } vm)
        {
            return;
        }

        var control = e.KeyModifiers.HasFlag(KeyModifiers.Control);
        var shift = e.KeyModifiers.HasFlag(KeyModifiers.Shift);
        switch (e.Key)
        {
            case Key.S when control:
                vm.SaveCommand.Execute(null);
                e.Handled = true;
                break;
            case Key.B when control:
                ApplyFormat(editor, MarkdownEditing.Bold, editor.SelectionStart, editor.SelectionEnd - editor.SelectionStart);
                e.Handled = true;
                break;
            case Key.I when control:
                ApplyFormat(editor, MarkdownEditing.Italic, editor.SelectionStart, editor.SelectionEnd - editor.SelectionStart);
                e.Handled = true;
                break;
            case Key.K when control:
                ApplyFormat(editor, MarkdownEditing.Link, editor.SelectionStart, editor.SelectionEnd - editor.SelectionStart);
                e.Handled = true;
                break;
            case Key.Tab:
                ApplyEditResult(editor, MarkdownEditing.Indent(
                    editor.Text ?? string.Empty,
                    editor.SelectionStart,
                    editor.SelectionEnd - editor.SelectionStart,
                    outdent: shift));
                e.Handled = true;
                break;
            case Key.Enter when !control && !shift:
                if (MarkdownEditing.ContinueList(editor.Text ?? string.Empty, editor.CaretIndex) is not { } next)
                {
                    break;
                }

                editor.Text = next.NewText;
                editor.CaretIndex = next.NewCaret;
                e.Handled = true;
                break;
        }
    }

    private void OnFormatClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { Tag: string format })
        {
            return;
        }

        // Focus has already moved to the button here, so the live selection is cleared
        // (0,0) — the tracked values from the editor's focused state are the real ones.
        var editor = _focusedEditor?.IsVisible == true ? _focusedEditor : GetActiveEditor();
        if (editor is null)
        {
            return;
        }

        var start = editor == _focusedEditor ? _selStart : editor.SelectionStart;
        var length = editor == _focusedEditor ? _selLength : editor.SelectionEnd - editor.SelectionStart;
        ApplyFormat(editor, format, start, length);
    }

    private TextBox? GetActiveEditor()
    {
        var full = this.FindControl<TextBox>("NoteEditor");
        var split = this.FindControl<TextBox>("NoteEditorSplit");
        if (full?.IsFocused == true)
        {
            return full;
        }

        if (split?.IsFocused == true)
        {
            return split;
        }

        return split?.IsVisible == true ? split : full;
    }

    // ---- interactive markdown preview ----

    /// <summary>The Markwing preview raises toggles by source line; the VM flips the
    /// line in its own note text (the source of truth) and the preview re-renders.
    /// The editor caret is preserved across the round-trip so typing can resume.</summary>
    private void OnTaskToggled(object? sender, TaskToggledEventArgs e)
    {
        if (ViewModel is not { } vm)
        {
            return;
        }

        var editor = GetActiveEditor();
        var caret = editor?.CaretIndex ?? -1;
        vm.ToggleTaskAtLine(e.Line);
        if (editor is not null && caret >= 0)
        {
            editor.CaretIndex = Math.Min(caret, (editor.Text ?? string.Empty).Length);
        }
    }

    private static void ApplyFormat(TextBox editor, string format, int start, int length)
    {
        var result = MarkdownEditing.Apply(format, editor.Text ?? string.Empty, start, length);
        ApplyEditResult(editor, result);
    }

    private static void ApplyEditResult(TextBox editor, MarkdownEditResult result)
    {
        editor.Text = result.Text;
        editor.SelectionStart = result.SelectionStart;
        editor.SelectionEnd = result.SelectionStart + result.SelectionLength;
        editor.Focus();
    }

    private void OnBackClick(object? sender, RoutedEventArgs e)
        => BackRequested?.Invoke(this, EventArgs.Empty);

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

    /// <summary>Minimal IObserver adapter — the BCL's Action-based Subscribe extension
    /// is not part of the framework surface this project resolves.</summary>
    private sealed class CaretObserver<T> : IObserver<T>
    {
        private readonly Action<T> _onNext;

        public CaretObserver(Action<T> onNext) => _onNext = onNext;

        public void OnCompleted()
        {
        }

        public void OnError(Exception error)
        {
        }

        public void OnNext(T value) => _onNext(value);
    }
}
