using Avalonia;
using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Input;
using Avalonia.Media;
using Avalonia.VisualTree;
using ColorTextBlock.Avalonia;
using AvaloniaPath = Avalonia.Controls.Shapes.Path;
using Markdown.Avalonia;
using MarkdownEngine = Markdown.Avalonia.Markdown;
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
        ConnectPreview("SplitPreview");
        ConnectPreview("PreviewOnly");
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

    /// <summary>Drives a preview viewer from the note text instead of a binding: the
    /// render must be preceded by populating CascadeResources with one checkbox
    /// control per task anchor, because the parse consumes them synchronously. The
    /// attach hook re-runs the same dance — the viewer re-parses on re-attach.</summary>
    private void ConnectPreview(string viewerName)
    {
        if (this.FindControl<MarkdownScrollViewer>(viewerName) is not { } viewer
            || DataContext is not NotesPageViewModel vm)
        {
            return;
        }

        // The viewer paints selection bands on an overlay canvas stacked above the
        // document; its rectangles survive unselect and intercept clicks, making a
        // selected line's checkbox untoggleable. Input-transparent fixes that while
        // the bands keep rendering. The canvas has no logical parent — match it by
        // its visual chain (canvas → wrapper → the viewer's internal ScrollViewer).
        foreach (var canvas in viewer.GetVisualDescendants().OfType<Canvas>())
        {
            if (canvas.GetVisualParent() is Control wrapper
                && wrapper.GetVisualParent() is ScrollViewer internalScroll
                && ReferenceEquals(internalScroll.GetVisualParent(), viewer))
            {
                canvas.IsHitTestVisible = false;
            }
        }

        viewer.AttachedToVisualTree += (_, _) => RenderPreview(viewer, vm);
        vm.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(NotesPageViewModel.NoteText))
            {
                RenderPreview(viewer, vm);
            }
        };
    }

    private void RenderPreview(MarkdownScrollViewer viewer, NotesPageViewModel vm)
    {
        if (viewer.Engine is not MarkdownEngine engine)
        {
            return;
        }

        var (preview, tasks) = MarkdownEditing.BuildPreview(vm.NoteText);
        var resources = engine.CascadeResources.Owner;
        resources.Clear();
        foreach (var task in tasks)
        {
            resources[$"mtask-{task.Line}"] = CreateTaskCheckbox(vm, task.Line, task.IsChecked);
        }

        viewer.Markdown = preview;
    }

    /// <summary>A plain hand-drawn box rather than a CheckBox: the checkbox sits inside
    /// the markdown viewer's template tree where theme resolution is unpredictable, and
    /// state is owned by the source text anyway — the box is a clickable picture.</summary>
    private Border CreateTaskCheckbox(NotesPageViewModel vm, int line, bool isChecked)
    {
        const string accent = "#8A5CF5";
        var tick = new AvaloniaPath
        {
            Data = Geometry.Parse("M 3,7.6 L 6.2,10.8 L 12,4.4"),
            Stroke = new SolidColorBrush(Color.Parse("#FFFFFF")),
            StrokeThickness = 1.8,
            IsVisible = isChecked,
        };
        var box = new Border
        {
            Width = 15,
            Height = 15,
            CornerRadius = new CornerRadius(3.5),
            BorderThickness = new Thickness(1.4),
            BorderBrush = new SolidColorBrush(Color.Parse(isChecked ? accent : "#8A8A8A")),
            Background = isChecked ? new SolidColorBrush(Color.Parse(accent)) : Brushes.Transparent,
            Cursor = new Cursor(StandardCursorType.Hand),
            Child = tick,
        };
        box.PointerEntered += (_, _) =>
        {
            if (!tick.IsVisible)
            {
                box.BorderBrush = new SolidColorBrush(Color.Parse("#C9B8F8"));
            }
        };
        box.PointerExited += (_, _) =>
        {
            if (!tick.IsVisible)
            {
                box.BorderBrush = new SolidColorBrush(Color.Parse("#8A8A8A"));
            }
        };
        box.PointerPressed += (_, e) =>
        {
            e.Handled = true;
            var editor = GetActiveEditor();
            var caret = editor?.CaretIndex ?? -1;
            vm.ToggleTaskAtLine(line);
            if (editor is not null && caret >= 0)
            {
                editor.CaretIndex = Math.Min(caret, (editor.Text ?? string.Empty).Length);
            }
        };
        box.AttachedToVisualTree += (_, _) => HideListMarker(box);
        return box;
    }

    /// <summary>Task items keep the list structure for indentation, but their bullet
    /// glyph is noise next to the checkbox: hide the row's marker column cell once
    /// the checkbox lands in the visual tree.</summary>
    private static void HideListMarker(Visual start)
    {
        var child = start;
        var node = start.GetVisualParent();
        while (node is not null)
        {
            if (node is Grid { Classes.Count: > 0 } grid && grid.Classes.Contains("List"))
            {
                var row = Grid.GetRow((Control)child);
                foreach (var marker in grid.Children)
                {
                    if (Grid.GetRow(marker) == row && Grid.GetColumn(marker) == 0)
                    {
                        marker.IsVisible = false;
                    }
                }

                return;
            }

            child = node;
            node = node.GetVisualParent();
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
