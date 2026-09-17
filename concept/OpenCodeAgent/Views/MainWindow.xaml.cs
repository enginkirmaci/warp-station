using System.Diagnostics;
using System.IO;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Input.Platform;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Media.Imaging;
using Avalonia.Platform.Storage;
using Avalonia.VisualTree;
using Avalonia.Controls.Primitives;
using OpenCodeAgent.Models;
using OpenCodeAgent.Services;
using OpenCodeAgent.ViewModels;
using SukiUI.Controls;

namespace OpenCodeAgent.Views;

public partial class MainWindow : SukiWindow
{
    private readonly MainViewModel _vm = new();
    private ScrollViewer? _chatScroll;
    private Button? _jumpLatest;
    private bool _atBottom = true;

    public MainWindow()
    {
        InitializeComponent();
        DataContext = _vm;
        _chatScroll = this.FindControl<ScrollViewer>("ChatScroll");
        _jumpLatest = this.FindControl<Button>("JumpLatest");
        // an AcceptsReturn TextBox consumes Enter before bubbling; tunnel to send first
        this.FindControl<TextBox>("InputBox")?
            .AddHandler(InputElement.KeyDownEvent, OnInputKeyDown, RoutingStrategies.Tunnel);
        if (_chatScroll is not null)
            _chatScroll.ScrollChanged += OnChatScrollChanged;
    }

    private void InitializeComponent() => AvaloniaXamlLoader.Load(this);

    // ScrollChanged carries the post-layout extent, so pinning is decided entirely here:
    // extent growth under a pinned view is streaming content arriving, never a scroll-up.
    private void OnChatScrollChanged(object? sender, ScrollChangedEventArgs e)
    {
        if (_chatScroll is null)
            return;
        if (Math.Abs(e.OffsetDelta.Y) > 0.5)
        {
            // real offset moves: user scrolling, or the re-stick below landing at the bottom
            _atBottom = _chatScroll.Offset.Y + _chatScroll.Viewport.Height >= _chatScroll.Extent.Height - 1;
        }
        else if (_atBottom && e.ExtentDelta.Y > 0.5)
        {
            _chatScroll.ScrollToEnd();
        }
        if (_jumpLatest is not null)
            _jumpLatest.IsVisible = !_atBottom && _vm.HasMessages;
    }

    private void OnJumpLatestClicked(object? sender, RoutedEventArgs e)
    {
        _atBottom = true;
        _chatScroll?.ScrollToEnd();
        if (_jumpLatest is not null)
            _jumpLatest.IsVisible = false;
    }

    private void OnExampleClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { Content: string prompt })
            return;
        _vm.LoadIntoInput(prompt);
        this.FindControl<TextBox>("InputBox")?.Focus();
    }

    private void OnInputKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.V && e.KeyModifiers.HasFlag(KeyModifiers.Control))
        {
            // let the TextBox's own text paste run; an image clipboard no-ops in it
            _ = PasteFromClipboardAsync();
            return;
        }
        if (e.Key != Key.Enter || e.KeyModifiers.HasFlag(KeyModifiers.Shift))
            return;
        e.Handled = true;
        _vm.SendCommand.Execute(null);
    }

    /// <summary>Ctrl+V: attach a clipboard image; plain text was already pasted by the TextBox itself.</summary>
    private async Task PasteFromClipboardAsync()
    {
        if (TopLevel.GetTopLevel(this)?.Clipboard is not { } clipboard)
            return;
        try
        {
            var bitmap = await clipboard.TryGetBitmapAsync();
            if (bitmap is not null)
            {
                using var ms = new MemoryStream();
                bitmap.Save(ms, new PngBitmapEncoderOptions());
                _vm.AttachImage(ms.ToArray());
                return;
            }
            var file = await clipboard.TryGetFileAsync();
            if (file is IStorageFile storageFile && storageFile.Name is { } name &&
                Path.GetExtension(name) is { Length: > 0 } ext &&
                new[] { ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp" }.Contains(ext, StringComparer.OrdinalIgnoreCase))
            {
                await using var stream = await storageFile.OpenReadAsync();
                using var decoded = new Bitmap(stream);
                using var png = new MemoryStream();
                decoded.Save(png, new PngBitmapEncoderOptions());
                _vm.AttachImage(png.ToArray());
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[paste] {ex.Message}");
        }
    }

    private async void OnCopyMessage(object? sender, RoutedEventArgs e)
    {
        var text = (sender as Control)?.DataContext switch
        {
            UserMessageItem u => u.Text,
            AssistantTextItem a => a.Text,
            ReasoningItem r => r.Text,
            SystemNoteItem note => note.Text,
            ToolItem t => string.Join(" ", new[] { t.Tool, t.Title }.Where(s => s.Length > 0)),
            PermissionItem p => $"{p.Title}\n{p.Detail}",
            _ => null,
        };
        if (sender is Control { DataContext: ImageItem image } && TopLevel.GetTopLevel(this) is { Clipboard: { } cb2 })
        {
            await cb2.SetBitmapAsync(image.Preview);
            return;
        }
        if (string.IsNullOrEmpty(text) || TopLevel.GetTopLevel(this) is not { Clipboard: { } clipboard })
            return;
        await clipboard.SetTextAsync(text);
    }

    private void OnRenameClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            _vm.BeginRename(session);
    }

    // ---- file drag & drop onto the transcript or the input card ----

    private void OnFileDragOver(object? sender, DragEventArgs e)
    {
        if (!e.DataTransfer.Contains(DataFormat.File))
            return;
        e.DragEffects = DragDropEffects.Copy;
        e.Handled = true;
    }

    private async void OnFileDrop(object? sender, DragEventArgs e)
    {
        if (!e.DataTransfer.Contains(DataFormat.File))
            return;
        var files = e.DataTransfer.TryGetFiles();
        if (files is null || files.Length == 0)
            return;
        e.Handled = true;
        foreach (var file in files)
            await AttachDroppedFileAsync(file);
    }

    private async Task AttachDroppedFileAsync(IStorageItem item)
    {
        if (item is not IStorageFile file)
            return;
        try
        {
            await using var stream = await file.OpenReadAsync();
            using var ms = new MemoryStream();
            await stream.CopyToAsync(ms);
            var data = ms.ToArray();
            if (data.Length > 512 * 1024)
            {
                _vm.Announce($"{file.Name} is over 512 KB — skipped.", error: true);
                return;
            }
            _vm.AttachFile(file.Name, MimeFor(Path.GetExtension(file.Name)), data);
        }
        catch (Exception ex)
        {
            _vm.Announce($"Could not attach {file.Name}: {ex.Message}", error: true);
        }
    }

    private static string MimeFor(string ext) => ext.ToLowerInvariant() switch
    {
        ".png" => "image/png",
        ".jpg" or ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".webp" => "image/webp",
        ".bmp" => "image/bmp",
        ".pdf" => "application/pdf",
        _ => "text/plain",
    };

    // ---- session context menu: share ----

    private async void OnShareClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is not SessionItem session)
            return;
        var url = await _vm.ShareSessionAsync(session);
        if (url is null)
            return;
        if (TopLevel.GetTopLevel(this)?.Clipboard is { } clipboard)
            await clipboard.SetTextAsync(url);
        _vm.Announce($"Share link copied: {url}");
    }

    private async void OnUnshareClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            await _vm.UnshareSessionAsync(session);
    }

    // ---- transcript + prompts flyout helpers ----

    private void OnEditMessageClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is not UserMessageItem message)
            return;
        _vm.LoadIntoInput(message.Text);
        this.FindControl<TextBox>("InputBox")?.Focus();
    }

    private async void OnCommandClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: CommandInfo command })
            return;
        HideHostFlyout(sender as Control);
        await _vm.RunCommandAsync(command);
    }

    private void OnPromptUseClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: string prompt })
            return;
        HideHostFlyout(sender as Control);
        _vm.LoadIntoInput(prompt);
        this.FindControl<TextBox>("InputBox")?.Focus();
    }

    // x:Name fields are never assigned by the hand-written InitializeComponent; walk to the host popup instead
    private static void HideHostFlyout(Control? control)
    {
        if (control?.GetVisualAncestors().OfType<Popup>().FirstOrDefault() is { } popup)
            popup.IsOpen = false;
    }

    private void OnPinClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            _vm.TogglePin(session);
    }

    private void OnUnpinClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            _vm.TogglePin(session);
    }

    private void OnDeleteClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            _ = _vm.DeleteSessionCommand.ExecuteAsync(session);
    }

    private void OnRenameKeyDown(object? sender, KeyEventArgs e)
    {
        if ((sender as Control)?.DataContext is not SessionItem session)
            return;
        if (e.Key == Key.Enter)
        {
            e.Handled = true;
            _ = _vm.CommitRenameCommand.ExecuteAsync(session);
        }
        else if (e.Key == Key.Escape)
        {
            e.Handled = true;
            _vm.CancelRename(session);
        }
    }

    private void OnRenameLostFocus(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session && session.IsEditing)
            _ = _vm.CommitRenameCommand.ExecuteAsync(session);
    }

    private async void OnAddWorkspaceClicked(object? sender, RoutedEventArgs e)
    {
        var folders = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Workspace folder for opencode",
            AllowMultiple = false,
        });
        if (folders.Count > 0 && folders[0].Path is { IsAbsoluteUri: true } path)
            _vm.AddWorkspace(path.LocalPath);
    }

    // ---- workspace context menu ----

    private void OnWorkspaceStartClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is WorkspaceItem ws)
            _ = _vm.StartWorkspaceCommand.ExecuteAsync(ws);
    }

    private void OnWorkspaceStopClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is WorkspaceItem ws)
            _vm.StopWorkspaceCommand.Execute(ws);
    }

    private void OnWorkspaceRemoveClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is WorkspaceItem ws)
            _vm.RemoveWorkspace(ws);
    }

    private void OnUnlinkClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            _vm.RemoveFromWorkspace(session);
    }

    private void OnRunningClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is RunningAgentItem running)
            _ = _vm.OpenRunningAsync(running);
    }

    protected override void OnClosing(WindowClosingEventArgs e)
    {
        _vm.Shutdown();
        base.OnClosing(e);
    }
}
