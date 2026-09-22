using System.Globalization;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Media;
using Avalonia.VisualTree;
using MarkdownViewerKit;
using Tools.Helpers;
using Tools.Library.Services.Abstractions;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

/// <summary>Code-behind of the Notes page: navigation back, the editor's Ctrl+S and
/// markdown keys, the formatting toolbar, selection-driven status readout, the search
/// box's Enter/Escape, search-result opening, the new note/folder flyouts and the live
/// mode's click-to-edit block overlay. Text transforms live in
/// <see cref="MarkdownEditing"/>, state in <see cref="NotesPageViewModel"/>.</summary>
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
        foreach (var name in new[] { "NoteEditor", "NoteEditorSplit", "LiveBlockEditor" })
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
        if (DataContext as NotesPageViewModel is not { } vm)
        {
            return;
        }

        // The block editor's caret indexes into its block slice; report it as a
        // document offset so the status bar stays meaningful.
        var caret = editor == _liveEditor && _liveSpanStart >= 0 ? _liveSpanStart + editor.CaretIndex : editor.CaretIndex;
        vm.UpdateCursorPosition(caret);
    }

    private void OnEditorKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Source is not TextBox { Name: "NoteEditor" or "NoteEditorSplit" or "LiveBlockEditor" } editor
            || ViewModel is not { } vm)
        {
            return;
        }

        var control = e.KeyModifiers.HasFlag(KeyModifiers.Control);
        var shift = e.KeyModifiers.HasFlag(KeyModifiers.Shift);
        var liveOpen = editor == _liveEditor && _liveEditorHost?.IsVisible == true;
        switch (e.Key)
        {
            case Key.Up when liveOpen && AtFirstLine(editor):
                NavigateLiveBlock(-1);
                e.Handled = true;
                break;
            case Key.Down when liveOpen && AtLastLine(editor):
                NavigateLiveBlock(1);
                e.Handled = true;
                break;
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
        if (_liveEditorHost?.IsVisible == true && _liveEditor is { } live)
        {
            return live;
        }

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

    // ---- live mode: click-to-edit blocks ----

    private TextBox? _liveEditor;
    private Border? _liveEditorHost;
    private ScrollViewer? _liveScroll;
    private MarkdownViewer? _liveViewer;
    private Control? _liveBlockControl;
    private int _liveSpanStart = -1;
    private int _liveSpanEnd = -1;
    private string? _liveOriginal;
    private bool _liveCommitting;

    // The host border (1px) plus the editor's padding (6,4): the card is grown
    // outward by these so the raw text lines up with the rendered text origin.
    private const double ChromeX = 7;
    private const double ChromeY = 5;

    /// <summary>Click on the rendered note: commits a pending block edit, then opens
    /// the source editor over the clicked block. The engine's top-level rendered
    /// children align 1:1 with MarkdownBlocks for notes-shaped documents; a count
    /// mismatch (structures the parser does not model) makes the click a no-op.</summary>
    private void OnLivePreviewPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        if (sender is not MarkdownViewer viewer || ViewModel is not { } vm)
        {
            return;
        }

        CommitLiveBlock();

        var doc = FindDocumentPanel(viewer);
        if (doc is null)
        {
            return;
        }

        var point = e.GetCurrentPoint(doc).Position;
        for (var i = 0; i < doc.Children.Count; i++)
        {
            if (doc.Children[i] is not Control { } child || !child.Bounds.Contains(point))
            {
                continue;
            }

            var blocks = MarkdownBlocks.Parse(vm.NoteText);
            if (blocks.Count != doc.Children.Count || i >= blocks.Count)
            {
                return;
            }

            OpenLiveBlockEditor(vm, viewer, child, doc, blocks[i], point);
            e.Handled = true;
            return;
        }

        e.Handled = true;
    }

    private void OpenLiveBlockEditor(NotesPageViewModel vm, MarkdownViewer viewer, Control blockControl, Panel layer, MarkdownBlock block, Point? clickPoint)
    {
        _liveEditor ??= this.FindControl<TextBox>("LiveBlockEditor");
        _liveEditorHost ??= this.FindControl<Border>("LiveBlockEditorHost");
        if (_liveEditor is not { } editor || _liveEditorHost is not { } host || host.Parent is not Panel overlayLayer)
        {
            return;
        }

        _liveSpanStart = block.Start;
        _liveSpanEnd = block.End;
        _liveOriginal = vm.NoteText[block.Start..block.End];
        _liveBlockControl = blockControl;
        _liveViewer = viewer;

        editor.Text = _liveOriginal;
        ApplyLiveBlockTypography(editor, viewer, block, _liveOriginal);

        host.IsVisible = true;
        host.IsHitTestVisible = true;
        PositionLiveEditorHost(blockControl, overlayLayer, blockControl.TranslatePoint(default, overlayLayer));

        editor.LostFocus += OnLiveEditorLostFocus;
        editor.Focus();
        if (clickPoint is { } point)
        {
            editor.CaretIndex = MapLiveCaret(editor, _liveOriginal, point - (Vector)blockControl.Bounds.Position, blockControl.Bounds.Height);
        }

        // The editor is positioned in viewport space; scrolling must follow the
        // block instead of leaving the card behind.
        _liveScroll = viewer.GetVisualDescendants().OfType<ScrollViewer>().FirstOrDefault();
        if (_liveScroll is not null)
        {
            _liveScroll.ScrollChanged += OnLiveScrollChanged;
        }
    }

    /// <summary>Sizes the card to hug the block: grown outward by the chrome so the
    /// editor's text origin lands exactly on the block's text origin, height fitted
    /// to the raw source and capped to the visible layer.</summary>
    private void PositionLiveEditorHost(Control blockControl, Panel overlayLayer, Point? originInLayer = null)
    {
        if (_liveEditorHost is not { } host || _liveEditor is not { } editor)
        {
            return;
        }

        if ((originInLayer ?? blockControl.TranslatePoint(default, overlayLayer)) is not { } origin)
        {
            return;
        }

        var left = Math.Max(0, origin.X - ChromeX);
        var top = Math.Max(0, origin.Y - ChromeY);
        var width = Math.Max(60, Math.Min(blockControl.Bounds.Width + ChromeX * 2, overlayLayer.Bounds.Width - left));

        // Measure at the width the text will actually wrap at (host minus border
        // minus editor padding), or wrapping paragraphs report a single line. The
        // card also grows to cover the whole block so no rendered row peeks out
        // under shorter raw source.
        editor.Measure(new Size(width - ChromeX * 2, double.PositiveInfinity));
        var contentHeight = Math.Max(editor.DesiredSize.Height, blockControl.Bounds.Height);
        var height = Math.Max(32, Math.Min(contentHeight + ChromeY * 2, overlayLayer.Bounds.Height - top));

        host.Margin = new Thickness(left, top, 0, 0);
        host.Width = width;
        host.Height = height;
    }

    /// <summary>Splices the edited slice back when the span still holds the text the
    /// editor opened with; false when the edit was discarded (span invalidated by a
    /// task toggle or note switch since opening).</summary>
    private bool CommitLiveBlock()
    {
        if (_liveCommitting || _liveEditorHost is not { } host || !host.IsVisible)
        {
            return false;
        }

        _liveCommitting = true;
        try
        {
            if (_liveScroll is not null)
            {
                _liveScroll.ScrollChanged -= OnLiveScrollChanged;
                _liveScroll = null;
            }

            if (_liveEditor is { } editor)
            {
                editor.LostFocus -= OnLiveEditorLostFocus;
            }

            host.IsVisible = false;
            host.IsHitTestVisible = false;
            _liveBlockControl = null;
            _liveViewer = null;

            var (start, end, original, text) = (_liveSpanStart, _liveSpanEnd, _liveOriginal, _liveEditor?.Text ?? string.Empty);
            _liveSpanStart = -1;
            _liveSpanEnd = -1;
            _liveOriginal = null;

            // Only splice when the span still holds the text the editor opened with
            // (a task toggle or note switch since opening invalidates it — discard).
            if (ViewModel is not { } vm
                || original is null
                || start < 0
                || end > vm.NoteText.Length
                || !string.Equals(vm.NoteText[start..end], original, StringComparison.Ordinal))
            {
                return false;
            }

            if (text != original)
            {
                vm.ReplaceTextRange(start, end, text);
            }

            return true;
        }
        finally
        {
            _liveCommitting = false;
        }
    }

    /// <summary>Arrow navigation between blocks: commits the pending edit, then opens
    /// the adjacent block (Up on the first line / Down on the last line reach this).
    /// Spans are resolved against the post-commit parse via the pending length delta;
    /// off-screen targets are scrolled into view before the editor opens.</summary>
    private void NavigateLiveBlock(int direction)
    {
        if (ViewModel is not { } vm
            || _liveEditor is not { } editor
            || _liveEditorHost is not { } host
            || _liveSpanStart < 0
            || _liveSpanEnd < 0
            || _liveSpanEnd > vm.NoteText.Length)
        {
            return;
        }

        var spanStart = _liveSpanStart;
        var spanEnd = _liveSpanEnd;
        var delta = (editor.Text ?? string.Empty).Length - (spanEnd - spanStart);
        var viewer = _liveViewer;

        if (!CommitLiveBlock() || viewer is null)
        {
            return;
        }

        var blocks = MarkdownBlocks.Parse(vm.NoteText);
        var targetIndex = direction < 0
            ? IndexOfLast(blocks, spanStart)
            : IndexOfFirst(blocks, spanEnd + delta);
        if (targetIndex < 0 || targetIndex >= blocks.Count)
        {
            return;
        }

        var doc = FindDocumentPanel(viewer);
        if (doc is null || doc.Children.Count != blocks.Count)
        {
            return;
        }

        if (doc.Children[targetIndex] is not Control child)
        {
            return;
        }

        child.BringIntoView();
        viewer.UpdateLayout();
        OpenLiveBlockEditor(vm, viewer, child, doc, blocks[targetIndex], clickPoint: null);
        if (host.IsVisible)
        {
            editor.CaretIndex = direction > 0 ? 0 : (editor.Text ?? string.Empty).Length;
        }
    }

    /// <summary>Block indexes by span geometry: the previous block is the last one
    /// ending at or before the just-edited span (its source is untouched by the
    /// splice); the next one is the first starting at or after the replacement's
    /// end (spanEnd + pending delta in the new text).</summary>
    private static int IndexOfLast(IReadOnlyList<MarkdownBlock> blocks, int limit)
    {
        for (var i = blocks.Count - 1; i >= 0; i--)
        {
            if (blocks[i].End <= limit)
            {
                return i;
            }
        }

        return -1;
    }

    private static int IndexOfFirst(IReadOnlyList<MarkdownBlock> blocks, int limit)
    {
        for (var i = 0; i < blocks.Count; i++)
        {
            if (blocks[i].Start >= limit)
            {
                return i;
            }
        }

        return -1;
    }

    /// <summary>Caret sits on the block's first / last source line — the Up/Down
    /// boundary where arrow navigation hands over to the adjacent block.</summary>
    private static bool AtFirstLine(TextBox editor)
        => editor.Text is not { } text || !text[..editor.CaretIndex].Contains('\n');

    private static bool AtLastLine(TextBox editor)
        => editor.Text is not { } text || !text[Math.Max(0, editor.CaretIndex)..].Contains('\n');

    private void OnLiveEditorKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Escape)
        {
            CommitLiveBlock();
            e.Handled = true;
        }
    }

    private void OnLiveEditorLostFocus(object? sender, RoutedEventArgs e) => CommitLiveBlock();

    private void OnLiveScrollChanged(object? sender, ScrollChangedEventArgs e)
    {
        if (e.OffsetDelta == default)
        {
            return;
        }

        if (_liveBlockControl is not { } block || _liveEditorHost?.Parent is not Panel overlayLayer
            || block.TranslatePoint(default, overlayLayer) is not { } origin)
        {
            CommitLiveBlock();
            return;
        }

        PositionLiveEditorHost(block, overlayLayer, origin);
    }

    /// <summary>The engine's document stack: the StackPanel inside the scroll wrapper
    /// (the same engine-internal template shape the selection-canvas fix matches).</summary>
    private static Panel? FindDocumentPanel(MarkdownViewer viewer)
        => viewer.GetVisualDescendants()
            .OfType<StackPanel>()
            .FirstOrDefault(sp => sp.GetVisualParent()?.GetType().Name.Contains("Wrapper") == true);

    // Body/heading metrics mirror MarkdownTheme.axaml (the preview's typography);
    // the text brush resolves from the viewer so light/dark variants follow it.
    private static readonly FontFamily LiveMonoFamily = new("menlo,monaco,consolas,courier new,monospace");

    private static readonly (double Size, FontWeight Weight)[] HeadingScale =
    {
        (25.9, FontWeight.Bold),
        (23.4, FontWeight.SemiBold),
        (21.1, FontWeight.SemiBold),
        (19, FontWeight.SemiBold),
        (17.2, FontWeight.SemiBold),
        (16, FontWeight.SemiBold),
    };

    /// <summary>Types the block editor like the rendered block it covers: body 16px,
    /// the theme's heading scale (level from the source), mono 14px for code, and the
    /// preview's text brush. Non-code blocks inherit the viewer's font family — the
    /// same chain the rendered body text uses.</summary>
    private static void ApplyLiveBlockTypography(TextBox editor, MarkdownViewer viewer, MarkdownBlock block, string source)
    {
        var code = block.Kind == MarkdownBlockKind.FencedCode;
        var level = HeadingLevel(block, source);
        var (size, weight) = level is int heading && heading <= HeadingScale.Length
            ? HeadingScale[heading - 1]
            : (16.0, FontWeight.Normal);

        // Code takes the theme's mono stack; body blocks inherit the ambient family —
        // the same chain the rendered body text (no theme family setter) resolves.
        if (code)
        {
            editor.FontFamily = LiveMonoFamily;
        }
        else
        {
            editor.ClearValue(TextBox.FontFamilyProperty);
        }

        editor.FontSize = size;
        editor.FontWeight = weight;
        if (viewer.TryFindResource("MarkwingTextBrush", out var value) && value is IBrush brush)
        {
            editor.Foreground = brush;
        }
    }

    private static int? HeadingLevel(MarkdownBlock block, string source)
    {
        if (block.Kind == MarkdownBlockKind.SetextHeading)
        {
            var underline = source[(source.LastIndexOf('\n') + 1)..].TrimStart();
            return underline.StartsWith('=') ? 1 : 2;
        }

        if (block.Kind != MarkdownBlockKind.Heading)
        {
            return null;
        }

        var i = 0;
        while (i < source.Length && (source[i] == ' ' || source[i] == '\t'))
        {
            i++;
        }

        var level = 0;
        while (i + level < source.Length && source[i + level] == '#')
        {
            level++;
        }

        return Math.Clamp(level, 1, 6);
    }

    /// <summary>Estimates the caret from the click point: the block's rendered height
    /// spread over its source lines, the column by measuring line prefixes with the
    /// editor's own typeface (proportional fonts included). A heuristic — wrapped
    /// paragraphs can land a line off.</summary>
    private static int MapLiveCaret(TextBox editor, string text, Point relative, double renderedHeight)
    {
        var lines = text.Split('\n');
        if (lines.Length == 0)
        {
            return 0;
        }

        var typeface = new Typeface(editor.FontFamily);
        double MeasureWidth(string value)
        {
            var measured = new FormattedText(
                value,
                CultureInfo.CurrentCulture,
                FlowDirection.LeftToRight,
                typeface,
                editor.FontSize,
                null);
            return measured.Width;
        }

        var lineHeight = Math.Max(renderedHeight / lines.Length, 1);
        var line = Math.Clamp((int)(relative.Y / lineHeight), 0, lines.Length - 1);
        var lineText = lines[line];
        int lo = 0, hi = lineText.Length;
        while (lo < hi)
        {
            var mid = (lo + hi + 1) / 2;
            if (MeasureWidth(lineText[..mid]) <= relative.X)
            {
                lo = mid;
            }
            else
            {
                hi = mid - 1;
            }
        }

        return lines.Take(line).Sum(l => l.Length + 1) + lo;
    }

    // ---- interactive markdown preview ----

    /// <summary>The Markwing preview raises toggles by source line; the VM flips the
    /// line in its own note text (the source of truth) and the preview re-renders.
    /// A pending block edit commits first: the toggle rewrites NoteText and would
    /// invalidate the block span. The editor caret is preserved across the round-trip
    /// so typing can resume.</summary>
    private void OnTaskToggled(object? sender, TaskToggledEventArgs e)
    {
        if (ViewModel is not { } vm)
        {
            return;
        }

        CommitLiveBlock();

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
